//! Semantic mutations backed by `toml_edit` (preserving comments, key order and
//! unknown fields), contest composition, overrides, problem package file writes,
//! and atomic freeze/unfreeze operations (docs/product.md §2–3, docs/architecture.md §5, §7.2–7.3).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};

use toml_edit::{Array, ArrayOfTables, DocumentMut, Item, Table, TableLike, Value};
use xcpc_domain::{
    ContestItem, ContestMeta, ContestMetaPatch, DiffKind, LimitsPatch, MAX_CONTEST_PROBLEMS,
    OverridePatch, Patch, Problem, ProblemMeta, ProblemMetaPatch, Source, TemplateOptionValue,
    TemplateOptionsMap, TemplateOptionsPatch, UnfreezeProblemDiff, check_id, label_for_index,
};

use crate::{
    Contest, Error, Result, SnapshotTemplateArchive, WORKSPACE_FILE, Workspace, content_hash, io,
    parse_toml, write_atomic,
};

/// Default skeleton written to `statement.md` by [`Workspace::create_problem`].
const STATEMENT_SKELETON: &str = "\n## 输入格式\n\n## 输出格式\n\n## 提示\n";

/// Allowed root-level filenames for [`Workspace::write_problem_files`].
const ALLOWED_PROBLEM_ROOT_FILES: &[&str] = &[
    "problem.toml",
    "statement.md",
    "statement.zh.md",
    "statement.en.md",
    "editorial.md",
    // Setter-only notes (intent, approaches to break, verification log); never typeset.
    "notes.md",
    "checker.cpp",
    "validator.cpp",
    "interactor.cpp",
];

/// Allowed single-level subdirectories for [`Workspace::write_problem_files`].
const ALLOWED_PROBLEM_SUBDIRS: &[&str] = &["samples", "solutions", "generators", "tests"];

impl Workspace {
    // ---- Workspace-level template settings ---------------------------------------------

    /// Partially updates `workspace.toml`'s default `template` and `[template-options.<tpl>]`
    /// via `toml_edit`, preserving comments, key order, and unknown fields.
    pub fn update_workspace_template(
        &self,
        template: &Patch<String>,
        options: &BTreeMap<String, TemplateOptionsPatch>,
    ) -> Result<()> {
        self.check_writable(WORKSPACE_FILE)?;
        let raw = match self.read_text(WORKSPACE_FILE) {
            Ok(s) => s,
            Err(Error::NotFound(_)) => String::new(),
            Err(e) => return Err(e),
        };
        let mut doc = parse_doc(WORKSPACE_FILE, &raw)?;
        let root = doc.as_table_mut();

        match template {
            Patch::Keep => {}
            Patch::Set(name) => {
                let trimmed = name.trim();
                if trimmed.is_empty() {
                    root.remove("template");
                } else {
                    check_id(trimmed)?;
                    set_scalar(root, "template", Value::from(trimmed));
                }
            }
            Patch::Clear => {
                root.remove("template");
            }
        }
        if !options.is_empty() {
            apply_template_options_patch(root, "template-options", options, None)?;
        }

        let rendered = doc.to_string();
        let _: toml::Table = parse_toml(WORKSPACE_FILE, &rendered)?;
        self.write(WORKSPACE_FILE, rendered.as_bytes())
    }

    // ---- Problem mutations -------------------------------------------------------------

    /// Creates a new problem directory with `problem.toml`, a `statement.md` skeleton,
    /// and an empty `samples/` directory.
    pub fn create_problem(&self, id: &str, title: &str) -> Result<Problem> {
        check_id(id)?;
        let meta_rel = format!("problems/{id}/problem.toml");
        self.check_writable(&meta_rel)?;

        let meta_path = self.resolve(&meta_rel)?;
        if meta_path.exists() {
            let actual = fs::read(&meta_path)
                .map(|b| content_hash(&b))
                .unwrap_or_default();
            return Err(Error::Conflict {
                path: meta_rel,
                expected: String::new(),
                actual,
            });
        }

        let samples_rel = format!("problems/{id}/samples");
        let samples_path = self.resolve(&samples_rel)?;
        fs::create_dir_all(&samples_path).map_err(|e| io(&samples_rel, e))?;
        // Re-check after creating directories in case a symlink appeared in the chain.
        self.resolve(&samples_rel)?;

        let mut doc = DocumentMut::new();
        doc["title"] = toml_edit::value(title);
        let mut limits = Table::new();
        limits["time"] = toml_edit::value("1s");
        limits["memory"] = toml_edit::value("256MB");
        doc["limits"] = Item::Table(limits);

        self.write(&meta_rel, doc.to_string().as_bytes())?;
        self.write(
            &format!("problems/{id}/statement.md"),
            STATEMENT_SKELETON.as_bytes(),
        )?;
        self.load_problem(id)
    }

    /// Recursively duplicates `problems/<source_id>/` into `problems/<new_id>/` atomically.
    ///
    /// Rejects symlinks anywhere inside the source tree (`Error::PathEscape`), rejects
    /// read-only workspaces (`Error::ReadOnly`), missing source (`Error::NotFound`), and
    /// existing target (`Error::Conflict`). If `new_title` is non-empty, updates `title`
    /// in the copied `problem.toml` via `toml_edit`, preserving comments, key order,
    /// unknown fields, `[[source]]`, and `[template-options]`.
    pub fn duplicate_problem(
        &self,
        source_id: &str,
        new_id: &str,
        new_title: Option<&str>,
    ) -> Result<Problem> {
        if self.is_read_only() {
            return Err(Error::ReadOnly);
        }
        check_id(source_id)?;
        check_id(new_id)?;

        let src_dir_rel = format!("problems/{source_id}");
        let src_meta_rel = format!("{src_dir_rel}/problem.toml");
        let src_dir = self.resolve(&src_dir_rel)?;
        let src_meta = self.resolve(&src_meta_rel)?;
        let src_dir_meta = fs::symlink_metadata(&src_dir).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => Error::NotFound(format!("problem {source_id:?}")),
            _ => io(&src_dir_rel, e),
        })?;
        if src_dir_meta.file_type().is_symlink() {
            return Err(Error::PathEscape(src_dir_rel));
        }
        if !src_meta.is_file() {
            return Err(Error::NotFound(format!("problem {source_id:?}")));
        }

        let dst_dir_rel = format!("problems/{new_id}");
        let dst_dir = self.resolve(&dst_dir_rel)?;
        if fs::symlink_metadata(&dst_dir).is_ok() {
            let actual = fs::read(dst_dir.join("problem.toml"))
                .map(|b| content_hash(&b))
                .unwrap_or_default();
            return Err(Error::Conflict {
                path: dst_dir_rel,
                expected: String::new(),
                actual,
            });
        }

        let problems_root = self.resolve("problems")?;
        fs::create_dir_all(&problems_root).map_err(|e| io("problems", e))?;
        self.resolve("problems")?;

        static DUP_COUNTER: AtomicU64 = AtomicU64::new(0);
        let stage_name = format!(
            ".dup.tmp-{}-{}",
            std::process::id(),
            DUP_COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        let stage_dir = problems_root.join(&stage_name);

        let copy_result: Result<()> = (|| {
            copy_dir_no_symlinks(self, &src_dir_rel, &src_dir, &stage_dir)?;
            if let Some(title) = new_title.map(str::trim).filter(|s| !s.is_empty()) {
                let stage_meta_path = stage_dir.join("problem.toml");
                let dst_meta_rel = format!("{dst_dir_rel}/problem.toml");
                let raw = fs::read_to_string(&stage_meta_path).map_err(|e| io(&dst_meta_rel, e))?;
                let mut doc = parse_doc(&dst_meta_rel, &raw)?;
                set_scalar(doc.as_table_mut(), "title", Value::from(title));
                let rendered = doc.to_string();
                let _: ProblemMeta = parse_toml(&dst_meta_rel, &rendered)?;
                write_atomic(&stage_meta_path, rendered.as_bytes())
                    .map_err(|e| io(&dst_meta_rel, e))?;
            }
            Ok(())
        })();

        if let Err(err) = copy_result {
            let _ = fs::remove_dir_all(&stage_dir);
            return Err(err);
        }

        if let Err(e) = fs::rename(&stage_dir, &dst_dir) {
            let _ = fs::remove_dir_all(&stage_dir);
            return Err(io(&dst_dir_rel, e));
        }

        self.load_problem(new_id)
    }

    /// Partially updates `problems/<id>/problem.toml` via `toml_edit`, preserving
    /// comments, key order, and unknown fields.
    pub fn update_problem_meta(&self, id: &str, patch: &ProblemMetaPatch) -> Result<ProblemMeta> {
        check_id(id)?;
        let rel = format!("problems/{id}/problem.toml");
        self.check_writable(&rel)?;

        let raw = self.read_text(&rel).map_err(|e| match e {
            Error::NotFound(_) => Error::NotFound(format!("problem {id:?}")),
            other => other,
        })?;
        let mut doc = parse_doc(&rel, &raw)?;
        let root = doc.as_table_mut();

        if let Some(title) = &patch.title {
            set_scalar(root, "title", Value::from(title.as_str()));
        }
        match &patch.rating {
            Patch::Keep => {}
            Patch::Set(r) => set_scalar(root, "rating", Value::from(i64::from(*r))),
            Patch::Clear => {
                root.remove("rating");
            }
        }
        if let Some(tags) = &patch.tags {
            if tags.is_empty() {
                root.remove("tags");
            } else {
                let mut arr = Array::new();
                for tag in tags {
                    arr.push(tag.as_str());
                }
                set_scalar(root, "tags", Value::Array(arr));
            }
        }
        match &patch.author {
            Patch::Keep => {}
            Patch::Set(author) => set_scalar(root, "author", Value::from(author.as_str())),
            Patch::Clear => {
                root.remove("author");
            }
        }
        if !patch.limits.is_empty() {
            apply_limits_patch(root, &patch.limits);
        }
        if let Some(sources) = &patch.sources {
            apply_sources(root, sources);
        }
        if !patch.template_options.is_empty() {
            apply_template_options_patch(root, "template-options", &patch.template_options, None)?;
        }

        let rendered = doc.to_string();
        let meta: ProblemMeta = parse_toml(&rel, &rendered)?;
        self.write(&rel, rendered.as_bytes())?;
        Ok(meta)
    }

    /// Deletes a problem from `problems/<id>/`.
    ///
    /// Refuses with [`Error::Referenced`] if any **unfrozen** contest references it.
    /// Problems referenced only by frozen contests may be deleted because frozen
    /// contests render from their own `snapshot/`.
    pub fn delete_problem(&self, id: &str) -> Result<()> {
        check_id(id)?;
        if self.is_read_only() {
            return Err(Error::ReadOnly);
        }

        let dir_rel = format!("problems/{id}");
        let dir_path = self.resolve(&dir_rel)?;
        if !dir_path.join("problem.toml").is_file() {
            return Err(Error::NotFound(format!("problem {id:?}")));
        }

        let mut referencing = Vec::new();
        for cid in self.list_contests()? {
            let contest = self.load_contest(&cid)?;
            if !contest.meta.frozen && contest.meta.problems.iter().any(|item| item.id == id) {
                referencing.push(cid);
            }
        }
        if !referencing.is_empty() {
            return Err(Error::Referenced {
                id: id.to_owned(),
                contests: referencing,
            });
        }

        // Refuse deleting through a symlink.
        let meta = fs::symlink_metadata(&dir_path).map_err(|e| io(&dir_rel, e))?;
        if meta.file_type().is_symlink() {
            return Err(Error::PathEscape(dir_rel));
        }
        fs::remove_dir_all(&dir_path).map_err(|e| io(&dir_rel, e))
    }

    /// Atomically writes a set of whitelisted files into `problems/<id>/`, creating the
    /// problem if it does not exist yet (in which case `problem.toml` must be included).
    ///
    /// Strictly validates every path against the problem file whitelist and parses
    /// `problem.toml` before touching disk. When `expected_hash` is provided, checks
    /// the target file's current SHA-256 hash on disk before writing. Returns the new
    /// SHA-256 hash for every written relative path.
    pub fn write_problem_files(
        &self,
        id: &str,
        files: &BTreeMap<String, String>,
        expected_hash: Option<&str>,
    ) -> Result<BTreeMap<String, String>> {
        check_id(id)?;
        if self.is_read_only() {
            return Err(Error::ReadOnly);
        }

        // 1. Strict whitelist + sandbox validation before touching disk.
        for rel_in_problem in files.keys() {
            if !is_allowed_problem_file(rel_in_problem) {
                return Err(Error::DisallowedPath(rel_in_problem.clone()));
            }
            let ws_rel = format!("problems/{id}/{rel_in_problem}");
            self.check_writable(&ws_rel)?;
        }

        // 2. Validate problem.toml parseability and creation requirement.
        let meta_rel = format!("problems/{id}/problem.toml");
        let meta_exists = self.resolve(&meta_rel)?.is_file();
        if let Some(toml_text) = files.get("problem.toml") {
            let _: ProblemMeta = parse_toml(&meta_rel, toml_text)?;
        } else if !meta_exists {
            return Err(Error::Invalid {
                path: meta_rel,
                message: "creating a new problem requires problem.toml".to_owned(),
            });
        }

        // 3. Conflict detection when expected_hash is provided.
        if let Some(expected) = expected_hash
            && let Some(check_key) = pick_hash_check_target(files)
        {
            let ws_rel = format!("problems/{id}/{check_key}");
            let path = self.resolve(&ws_rel)?;
            let actual = match fs::read(&path) {
                Ok(bytes) => content_hash(&bytes),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
                Err(e) => return Err(io(&ws_rel, e)),
            };
            if actual != expected {
                return Err(Error::Conflict {
                    path: ws_rel,
                    expected: expected.to_owned(),
                    actual,
                });
            }
        }

        // 4. Write all files atomically and return their new SHA-256 hashes.
        let mut hashes = BTreeMap::new();
        for (rel_in_problem, content) in files {
            let ws_rel = format!("problems/{id}/{rel_in_problem}");
            let bytes = content.as_bytes();
            self.write(&ws_rel, bytes)?;
            hashes.insert(rel_in_problem.clone(), content_hash(bytes));
        }
        Ok(hashes)
    }

    /// Lists solution filenames under `problems/<id>/solutions/`, sorted.
    pub fn list_solutions(&self, id: &str) -> Result<Vec<String>> {
        check_id(id)?;
        let meta_rel = format!("problems/{id}/problem.toml");
        if !self.resolve(&meta_rel)?.is_file() {
            return Err(Error::NotFound(format!("problem {id:?}")));
        }
        let dir_rel = format!("problems/{id}/solutions");
        let dir_path = self.resolve(&dir_rel)?;
        let entries = match fs::read_dir(&dir_path) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(io(&dir_rel, e)),
        };
        let mut names = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| io(&dir_rel, e))?;
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };
            if name.starts_with('.') {
                continue;
            }
            let file_rel = format!("{dir_rel}/{name}");
            let file_path = self.resolve(&file_rel)?;
            if file_path.is_file() {
                names.push(name);
            }
        }
        names.sort();
        Ok(names)
    }

    /// Reads one solution file under `problems/<id>/solutions/<name>` inside the sandbox.
    pub fn read_solution(&self, id: &str, name: &str) -> Result<String> {
        check_id(id)?;
        if !is_safe_single_filename(name) {
            return Err(Error::PathEscape(name.to_owned()));
        }
        self.read_text(&format!("problems/{id}/solutions/{name}"))
    }

    // ---- Contest composition & overrides -----------------------------------------------

    /// Composes a contest (creating it if it does not exist yet): sets its ordered
    /// problem list (re-labelling `A`, `B`, `C`, …), preserves overrides/comments/unknown
    /// fields on kept problems, removes dropped problems along with their overrides, and
    /// optionally updates contest metadata via `toml_edit`.
    pub fn compose_contest(
        &self,
        id: &str,
        problem_ids: &[String],
        meta_patch: Option<&ContestMetaPatch>,
    ) -> Result<Contest> {
        check_id(id)?;
        let rel = format!("contests/{id}/contest.toml");
        self.check_writable(&rel)?;

        if problem_ids.len() > MAX_CONTEST_PROBLEMS {
            return Err(Error::TooManyProblems(problem_ids.len()));
        }

        let mut seen = HashSet::with_capacity(problem_ids.len());
        for pid in problem_ids {
            check_id(pid)?;
            if !seen.insert(pid.as_str()) {
                return Err(Error::Invalid {
                    path: rel.clone(),
                    message: format!("duplicate problem id {pid:?}"),
                });
            }
            let prob_meta = format!("problems/{pid}/problem.toml");
            if !self.resolve(&prob_meta)?.is_file() {
                return Err(Error::NotFound(format!("problem {pid:?}")));
            }
        }

        let mut doc = match self.read_text(&rel) {
            Ok(raw) => parse_doc(&rel, &raw)?,
            Err(Error::NotFound(_)) => {
                let mut d = DocumentMut::new();
                let default_name = meta_patch.and_then(|p| p.name.as_deref()).unwrap_or(id);
                d["name"] = toml_edit::value(default_name);
                d["language"] = toml_edit::value("chinese");
                d
            }
            Err(e) => return Err(e),
        };

        if let Some(patch) = meta_patch {
            apply_contest_meta_patch(doc.as_table_mut(), patch)?;
        }
        rewrite_contest_problem_tables(&mut doc, problem_ids);

        let rendered = doc.to_string();
        let _: ContestMeta = parse_toml(&rel, &rendered)?;
        self.write(&rel, rendered.as_bytes())?;
        self.load_contest(id)
    }

    /// Partially updates `contests/<id>/contest.toml` metadata via `toml_edit`,
    /// preserving comments, key order, `[[problem]]` entries, and unknown fields.
    pub fn update_contest_meta(&self, id: &str, patch: &ContestMetaPatch) -> Result<ContestMeta> {
        check_id(id)?;
        let rel = format!("contests/{id}/contest.toml");
        self.check_writable(&rel)?;

        let raw = self.read_text(&rel).map_err(|e| match e {
            Error::NotFound(_) => Error::NotFound(format!("contest {id:?}")),
            other => other,
        })?;
        let mut doc = parse_doc(&rel, &raw)?;
        apply_contest_meta_patch(doc.as_table_mut(), patch)?;

        let rendered = doc.to_string();
        let meta: ContestMeta = parse_toml(&rel, &rendered)?;
        self.write(&rel, rendered.as_bytes())?;
        Ok(meta)
    }

    /// Appends `problem_id` to an existing contest and assigns the next label (`A`..=`Z`).
    pub fn add_problem_to_contest(&self, contest_id: &str, problem_id: &str) -> Result<Contest> {
        check_id(contest_id)?;
        check_id(problem_id)?;
        let rel = format!("contests/{contest_id}/contest.toml");
        self.check_writable(&rel)?;

        let contest = self.load_contest(contest_id)?;
        let mut ids: Vec<String> = contest.meta.problems.iter().map(|p| p.id.clone()).collect();
        ids.push(problem_id.to_owned());
        self.compose_contest(contest_id, &ids, None)
    }

    /// Removes `problem_id` (and its overrides) from an existing contest and re-labels
    /// the remaining problems sequentially (`A`, `B`, `C`, …).
    pub fn remove_problem_from_contest(
        &self,
        contest_id: &str,
        problem_id: &str,
    ) -> Result<Contest> {
        check_id(contest_id)?;
        let rel = format!("contests/{contest_id}/contest.toml");
        self.check_writable(&rel)?;

        let contest = self.load_contest(contest_id)?;
        let before = contest.meta.problems.len();
        let remaining: Vec<String> = contest
            .meta
            .problems
            .iter()
            .filter(|p| p.id != problem_id)
            .map(|p| p.id.clone())
            .collect();
        if remaining.len() == before {
            return Err(Error::NotFound(format!(
                "problem {problem_id:?} in contest {contest_id:?}"
            )));
        }

        let raw = self.read_text(&rel)?;
        let mut doc = parse_doc(&rel, &raw)?;
        rewrite_contest_problem_tables(&mut doc, &remaining);
        let rendered = doc.to_string();
        let _: ContestMeta = parse_toml(&rel, &rendered)?;
        self.write(&rel, rendered.as_bytes())?;
        self.load_contest(contest_id)
    }

    /// Reorders an existing contest's problems to match `ordered_problem_ids` (must be a
    /// permutation of the current problems), preserving overrides and re-labelling `A..Z`.
    pub fn reorder_contest_problems(
        &self,
        contest_id: &str,
        ordered_problem_ids: &[String],
    ) -> Result<Contest> {
        check_id(contest_id)?;
        let rel = format!("contests/{contest_id}/contest.toml");
        self.check_writable(&rel)?;

        let contest = self.load_contest(contest_id)?;
        let current: HashSet<&str> = contest
            .meta
            .problems
            .iter()
            .map(|p| p.id.as_str())
            .collect();
        let next: HashSet<&str> = ordered_problem_ids.iter().map(String::as_str).collect();
        if ordered_problem_ids.len() != contest.meta.problems.len()
            || next.len() != ordered_problem_ids.len()
            || current != next
        {
            return Err(Error::Invalid {
                path: rel,
                message: "reorder list must be a permutation of the contest's current problems"
                    .to_owned(),
            });
        }

        let raw = self.read_text(&rel)?;
        let mut doc = parse_doc(&rel, &raw)?;
        rewrite_contest_problem_tables(&mut doc, ordered_problem_ids);
        let rendered = doc.to_string();
        let _: ContestMeta = parse_toml(&rel, &rendered)?;
        self.write(&rel, rendered.as_bytes())?;
        self.load_contest(contest_id)
    }

    /// Sets or clears metadata overrides (`title`, `limits`, `show-metadata`) for the
    /// problem at `label` inside `contest_id` via `toml_edit`.
    ///
    /// Automatically deletes any override key whose new value equals the library
    /// problem's original value (or the contest's default `show-metadata`), and deletes
    /// `[problem.limits]` once it becomes empty.
    pub fn set_override(
        &self,
        contest_id: &str,
        label: &str,
        patch: &OverridePatch,
    ) -> Result<Contest> {
        check_id(contest_id)?;
        let rel = format!("contests/{contest_id}/contest.toml");
        self.check_writable(&rel)?;

        let contest = self.load_contest(contest_id)?;
        let item = contest
            .meta
            .problems
            .iter()
            .find(|p| p.label == label)
            .ok_or_else(|| Error::NotFound(format!("label {label:?} in contest {contest_id:?}")))?;
        let problem = self.load_problem(&item.id)?;

        let raw = self.read_text(&rel)?;
        let mut doc = parse_doc(&rel, &raw)?;
        let aot = doc
            .get_mut("problem")
            .and_then(Item::as_array_of_tables_mut)
            .ok_or_else(|| Error::NotFound(format!("label {label:?} in contest {contest_id:?}")))?;
        let tbl = aot
            .iter_mut()
            .find(|t| t.get("label").and_then(Item::as_str) == Some(label))
            .ok_or_else(|| Error::NotFound(format!("label {label:?} in contest {contest_id:?}")))?;

        match &patch.title {
            Patch::Keep => {}
            Patch::Clear => {
                tbl.remove("title");
            }
            Patch::Set(new_title) => {
                if new_title == &problem.meta.title {
                    tbl.remove("title");
                } else {
                    set_scalar(tbl, "title", Value::from(new_title.as_str()));
                }
            }
        }

        match &patch.show_metadata {
            Patch::Keep => {}
            Patch::Clear => {
                tbl.remove("show-metadata");
            }
            Patch::Set(show) => {
                if *show == contest.meta.show_metadata {
                    tbl.remove("show-metadata");
                } else {
                    set_scalar(tbl, "show-metadata", Value::from(*show));
                }
            }
        }

        if !patch.limits.is_empty() {
            let effective = LimitsPatch {
                time: normalize_limit_override(
                    &patch.limits.time,
                    problem.meta.limits.time.as_deref(),
                ),
                memory: normalize_limit_override(
                    &patch.limits.memory,
                    problem.meta.limits.memory.as_deref(),
                ),
                input: normalize_limit_override(
                    &patch.limits.input,
                    problem.meta.limits.input.as_deref(),
                ),
                output: normalize_limit_override(
                    &patch.limits.output,
                    problem.meta.limits.output.as_deref(),
                ),
            };
            apply_limits_patch(tbl, &effective);
        }

        if !patch.options.is_empty() {
            apply_template_options_patch(
                tbl,
                "options",
                &patch.options,
                Some(&problem.meta.template_options),
            )?;
        }

        let rendered = doc.to_string();
        let _: ContestMeta = parse_toml(&rel, &rendered)?;
        self.write(&rel, rendered.as_bytes())?;
        self.load_contest(contest_id)
    }

    // ---- Freeze & Unfreeze -------------------------------------------------------------

    /// Low-level atomic freeze commit (without extra template archive).
    pub fn freeze_commit(
        &self,
        contest_id: &str,
        pdf_statements: &[u8],
        pdf_editorial: Option<&[u8]>,
    ) -> Result<Contest> {
        self.freeze_commit_with_template(contest_id, pdf_statements, pdf_editorial, None)
    }

    /// Low-level atomic freeze commit, optionally archiving the contest's resolved
    /// template metadata (`snapshot/template.toml`), custom template file
    /// (`snapshot/template.typ`), and `templates/preamble.typ` (`snapshot/preamble.typ`).
    pub fn freeze_commit_with_template(
        &self,
        contest_id: &str,
        pdf_statements: &[u8],
        pdf_editorial: Option<&[u8]>,
        template_archive: Option<&SnapshotTemplateArchive>,
    ) -> Result<Contest> {
        check_id(contest_id)?;
        if self.is_read_only() {
            return Err(Error::ReadOnly);
        }

        let contest = self.load_contest(contest_id)?;
        if contest.meta.archived {
            return Err(Error::Archived(contest_id.to_owned()));
        }
        if contest.meta.frozen {
            return Err(Error::AlreadyFrozen(contest_id.to_owned()));
        }

        // Pre-validate that every referenced problem exists and its files are readable.
        let mut staged_problems: Vec<(String, BTreeMap<String, Vec<u8>>)> =
            Vec::with_capacity(contest.meta.problems.len());
        for item in &contest.meta.problems {
            let lib_dir = format!("problems/{}", item.id);
            let meta_rel = format!("{lib_dir}/problem.toml");
            if !self.resolve(&meta_rel)?.is_file() {
                return Err(Error::NotFound(format!("problem {:?}", item.id)));
            }
            let mut files = self.collect_snapshot_files(&lib_dir)?;
            let raw_meta = self.read_text(&meta_rel)?;
            let merged_meta = apply_item_overrides_to_toml(&raw_meta, item, &meta_rel)?;
            files.insert("problem.toml".to_owned(), merged_meta.into_bytes());
            staged_problems.push((item.label.clone(), files));
        }

        let contest_dir_rel = format!("contests/{contest_id}");
        let contest_dir = self.resolve(&contest_dir_rel)?;
        let snapshot_rel = format!("{contest_dir_rel}/snapshot");
        let snapshot_dir = self.resolve(&snapshot_rel)?;

        static STAGE_COUNTER: AtomicU64 = AtomicU64::new(0);
        let stage_name = format!(
            ".snapshot.tmp-{}-{}",
            std::process::id(),
            STAGE_COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        let stage_dir = contest_dir.join(&stage_name);

        let stage_result: Result<()> = (|| {
            fs::create_dir_all(&stage_dir).map_err(|e| io(&snapshot_rel, e))?;
            for (label, files) in &staged_problems {
                let label_dir = stage_dir.join(label);
                fs::create_dir_all(&label_dir).map_err(|e| io(&snapshot_rel, e))?;
                for (rel_file, bytes) in files {
                    let target = label_dir.join(rel_file);
                    if let Some(parent) = target.parent() {
                        fs::create_dir_all(parent).map_err(|e| io(&snapshot_rel, e))?;
                    }
                    write_atomic(&target, bytes).map_err(|e| io(&snapshot_rel, e))?;
                }
            }
            let pdf_path = stage_dir.join(format!("{contest_id}.pdf"));
            write_atomic(&pdf_path, pdf_statements).map_err(|e| io(&snapshot_rel, e))?;
            if let Some(ed_pdf) = pdf_editorial {
                let ed_path = stage_dir.join(format!("{contest_id}-editorial.pdf"));
                write_atomic(&ed_path, ed_pdf).map_err(|e| io(&snapshot_rel, e))?;
            }
            if let Some(archive) = template_archive {
                let meta_text =
                    toml::to_string(&archive.meta).expect("snapshot template meta serializes");
                write_atomic(&stage_dir.join("template.toml"), meta_text.as_bytes())
                    .map_err(|e| io(&snapshot_rel, e))?;
                if let Some(tpl_bytes) = &archive.custom_template_bytes {
                    write_atomic(&stage_dir.join("template.typ"), tpl_bytes)
                        .map_err(|e| io(&snapshot_rel, e))?;
                }
                if let Some(pre_bytes) = &archive.preamble_bytes {
                    write_atomic(&stage_dir.join("preamble.typ"), pre_bytes)
                        .map_err(|e| io(&snapshot_rel, e))?;
                }
                if !archive.assets.is_empty() {
                    let assets_dir = stage_dir.join("assets");
                    fs::create_dir_all(&assets_dir).map_err(|e| io(&snapshot_rel, e))?;
                    for (filename, bytes) in &archive.assets {
                        write_atomic(&assets_dir.join(filename), bytes)
                            .map_err(|e| io(&snapshot_rel, e))?;
                    }
                }
            }
            Ok(())
        })();

        if let Err(err) = stage_result {
            let _ = fs::remove_dir_all(&stage_dir);
            return Err(err);
        }

        // Remove any leftover snapshot directory (contest is not frozen yet), then rename.
        if snapshot_dir.exists() {
            let meta = fs::symlink_metadata(&snapshot_dir).map_err(|e| io(&snapshot_rel, e))?;
            if meta.file_type().is_symlink() {
                let _ = fs::remove_dir_all(&stage_dir);
                return Err(Error::PathEscape(snapshot_rel));
            }
            if let Err(e) = fs::remove_dir_all(&snapshot_dir) {
                let _ = fs::remove_dir_all(&stage_dir);
                return Err(io(&snapshot_rel, e));
            }
        }
        if let Err(e) = fs::rename(&stage_dir, &snapshot_dir) {
            let _ = fs::remove_dir_all(&stage_dir);
            return Err(io(&snapshot_rel, e));
        }

        // Finally mark `frozen = true` in contest.toml via toml_edit.
        let contest_rel = format!("{contest_dir_rel}/contest.toml");
        let mark_result: Result<()> = (|| {
            let raw = self.read_text(&contest_rel)?;
            let mut doc = parse_doc(&contest_rel, &raw)?;
            set_scalar(doc.as_table_mut(), "frozen", Value::from(true));
            let rendered = doc.to_string();
            let _: ContestMeta = parse_toml(&contest_rel, &rendered)?;
            self.write(&contest_rel, rendered.as_bytes())
        })();

        if let Err(err) = mark_result {
            let _ = fs::remove_dir_all(&snapshot_dir);
            return Err(err);
        }

        self.load_contest(contest_id)
    }

    /// Byte-for-byte comparison between a frozen contest's `snapshot/<label>/` and the
    /// live library copies under `problems/<id>/`.
    ///
    /// Aligns `problem.toml` with the contest item's merged overrides so frozen overrides
    /// never trigger false-positive metadata diffs.
    pub fn unfreeze_diff(&self, contest_id: &str) -> Result<Vec<UnfreezeProblemDiff>> {
        check_id(contest_id)?;
        let contest = self.load_contest(contest_id)?;
        if !contest.meta.frozen {
            return Err(Error::NotFrozen(contest_id.to_owned()));
        }

        let mut diffs = Vec::new();
        for item in &contest.meta.problems {
            let snap_dir = format!("contests/{contest_id}/snapshot/{}", item.label);
            let lib_dir = format!("problems/{}", item.id);
            let lib_meta_rel = format!("{lib_dir}/problem.toml");

            if !self.resolve(&lib_meta_rel)?.is_file() {
                diffs.push(UnfreezeProblemDiff {
                    label: item.label.clone(),
                    problem_id: item.id.clone(),
                    kinds: vec![DiffKind::ProblemDeleted],
                    changed_files: Vec::new(),
                });
                continue;
            }

            let snap_files = self.collect_snapshot_files(&snap_dir)?;
            let mut lib_files = self.collect_snapshot_files(&lib_dir)?;
            if let Some(raw_bytes) = lib_files.get("problem.toml")
                && let Ok(raw_str) = std::str::from_utf8(raw_bytes)
                && let Ok(merged) = apply_item_overrides_to_toml(raw_str, item, &lib_meta_rel)
            {
                lib_files.insert("problem.toml".to_owned(), merged.into_bytes());
            }

            let all_keys: BTreeSet<&str> = snap_files
                .keys()
                .chain(lib_files.keys())
                .map(String::as_str)
                .collect();

            let mut kinds: BTreeSet<DiffKind> = BTreeSet::new();
            let mut changed_files = Vec::new();
            for key in all_keys {
                if snap_files.get(key) != lib_files.get(key) {
                    changed_files.push(key.to_owned());
                    if let Some(kind) = classify_snapshot_file(key) {
                        kinds.insert(kind);
                    }
                }
            }

            if !kinds.is_empty() {
                diffs.push(UnfreezeProblemDiff {
                    label: item.label.clone(),
                    problem_id: item.id.clone(),
                    kinds: kinds.into_iter().collect(),
                    changed_files,
                });
            }
        }

        Ok(diffs)
    }

    /// Sole unfreeze entry point (for GUI/CLI with user confirmation; never exposed to MCP).
    ///
    /// Sets `frozen = false` in `contest.toml` via `toml_edit` and removes `snapshot/`.
    pub fn unfreeze_contest(&self, contest_id: &str) -> Result<Contest> {
        check_id(contest_id)?;
        if self.is_read_only() {
            return Err(Error::ReadOnly);
        }

        let contest = self.load_contest(contest_id)?;
        if contest.meta.archived {
            return Err(Error::Archived(contest_id.to_owned()));
        }
        if !contest.meta.frozen {
            return Err(Error::NotFrozen(contest_id.to_owned()));
        }

        let contest_rel = format!("contests/{contest_id}/contest.toml");
        let raw = self.read_text(&contest_rel)?;
        let mut doc = parse_doc(&contest_rel, &raw)?;
        set_scalar(doc.as_table_mut(), "frozen", Value::from(false));
        let rendered = doc.to_string();
        let _: ContestMeta = parse_toml(&contest_rel, &rendered)?;

        // Write contest.toml first (bypassing only the frozen check; still sandboxed & read-only checked).
        self.write_unfreeze_bypass(&contest_rel, rendered.as_bytes())?;

        // Remove snapshot/ if present.
        let snapshot_rel = format!("contests/{contest_id}/snapshot");
        let snapshot_path = self.resolve(&snapshot_rel)?;
        if snapshot_path.exists() {
            let meta = fs::symlink_metadata(&snapshot_path).map_err(|e| io(&snapshot_rel, e))?;
            if meta.file_type().is_symlink() {
                return Err(Error::PathEscape(snapshot_rel));
            }
            fs::remove_dir_all(&snapshot_path).map_err(|e| io(&snapshot_rel, e))?;
        }

        self.load_contest(contest_id)
    }

    /// Archives or unarchives a contest by setting `archived = true` or removing the
    /// `archived` key in `contests/<id>/contest.toml` via `toml_edit`.
    ///
    /// Orthogonal to `frozen`, idempotent, and allowed on already-frozen or already-archived
    /// contests (still enforces `read_only` and workspace sandbox).
    pub fn set_contest_archived(&self, id: &str, archived: bool) -> Result<ContestMeta> {
        check_id(id)?;
        if self.is_read_only() {
            return Err(Error::ReadOnly);
        }
        let rel = format!("contests/{id}/contest.toml");
        let raw = self.read_text(&rel).map_err(|e| match e {
            Error::NotFound(_) => Error::NotFound(format!("contest {id:?}")),
            other => other,
        })?;
        let mut doc = parse_doc(&rel, &raw)?;
        if archived {
            set_scalar(doc.as_table_mut(), "archived", Value::from(true));
        } else {
            doc.as_table_mut().remove("archived");
        }
        let rendered = doc.to_string();
        let meta: ContestMeta = parse_toml(&rel, &rendered)?;
        self.write_unfreeze_bypass(&rel, rendered.as_bytes())?;
        Ok(meta)
    }

    /// Copies effective book-level and problem-level options from `source_tpl` into the
    /// `"local"` namespace in `contests/<contest_id>/contest.toml` when `"local"` is empty,
    /// so detaching a contest template preserves existing layout customizations.
    pub(crate) fn copy_contest_options_to_local(
        &self,
        contest_id: &str,
        source_tpl: &str,
    ) -> Result<()> {
        let rel = format!("contests/{contest_id}/contest.toml");
        let contest = self.load_contest(contest_id)?;
        let ws_opts = self
            .workspace_template_options()
            .remove(source_tpl)
            .unwrap_or_default();

        // Merge workspace defaults + contest overrides for `source_tpl` into `local` if `local` is empty.
        let mut book_local = BTreeMap::new();
        if contest
            .meta
            .template_options
            .get("local")
            .is_none_or(BTreeMap::is_empty)
        {
            for (k, v) in ws_opts {
                book_local.insert(k, Patch::Set(v));
            }
            if let Some(c_opts) = contest.meta.template_options.get(source_tpl) {
                for (k, v) in c_opts {
                    book_local.insert(k.clone(), Patch::Set(v.clone()));
                }
            }
        }

        // Collect per-problem option copies for `[[problem]]` entries whose `local` is empty.
        let mut prob_copies: HashMap<String, TemplateOptionsPatch> = HashMap::new();
        for item in &contest.meta.problems {
            if item.options.get("local").is_none_or(BTreeMap::is_empty)
                && let Some(src_opts) = item.options.get(source_tpl)
                && !src_opts.is_empty()
            {
                let patch: TemplateOptionsPatch = src_opts
                    .iter()
                    .map(|(k, v)| (k.clone(), Patch::Set(v.clone())))
                    .collect();
                prob_copies.insert(item.label.clone(), patch);
            }
        }

        if book_local.is_empty() && prob_copies.is_empty() {
            return Ok(());
        }

        let raw = self.read_text(&rel)?;
        let mut doc = parse_doc(&rel, &raw)?;
        if !book_local.is_empty() {
            let mut ns = BTreeMap::new();
            ns.insert("local".to_owned(), book_local);
            apply_template_options_patch(doc.as_table_mut(), "template-options", &ns, None)?;
        }
        if !prob_copies.is_empty()
            && let Some(aot) = doc
                .get_mut("problem")
                .and_then(Item::as_array_of_tables_mut)
        {
            for tbl in aot.iter_mut() {
                if let Some(label) = tbl.get("label").and_then(Item::as_str)
                    && let Some(patch) = prob_copies.remove(label)
                {
                    let mut ns = BTreeMap::new();
                    ns.insert("local".to_owned(), patch);
                    apply_template_options_patch(tbl, "options", &ns, None)?;
                }
            }
        }
        let rendered = doc.to_string();
        let _: ContestMeta = parse_toml(&rel, &rendered)?;
        self.write(&rel, rendered.as_bytes())
    }

    /// Collects the snapshot-scoped files (`problem.toml`, `statement*.md`, `editorial.md`,
    /// `samples/**`, `assets/**`) from `dir` as relative path -> exact bytes.
    fn collect_snapshot_files(&self, dir: &str) -> Result<BTreeMap<String, Vec<u8>>> {
        let mut out = BTreeMap::new();
        let base_path = self.resolve(dir)?;
        let entries = match fs::read_dir(&base_path) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
            Err(e) => return Err(io(dir, e)),
        };

        for entry in entries {
            let entry = entry.map_err(|e| io(dir, e))?;
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };
            if name.starts_with('.') {
                continue;
            }
            let rel = format!("{dir}/{name}");
            let resolved = self.resolve(&rel)?;
            if is_snapshot_root_file(&name) && resolved.is_file() {
                let bytes = fs::read(&resolved).map_err(|e| io(&rel, e))?;
                out.insert(name, bytes);
            } else if matches!(name.as_str(), "samples" | "assets") && resolved.is_dir() {
                self.collect_subtree_files(dir, &name, &mut out)?;
            }
        }
        Ok(out)
    }

    fn collect_subtree_files(
        &self,
        root_rel: &str,
        sub_rel: &str,
        out: &mut BTreeMap<String, Vec<u8>>,
    ) -> Result<()> {
        let full_rel = format!("{root_rel}/{sub_rel}");
        let path = self.resolve(&full_rel)?;
        let entries = match fs::read_dir(&path) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(io(&full_rel, e)),
        };
        for entry in entries {
            let entry = entry.map_err(|e| io(&full_rel, e))?;
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };
            if name.starts_with('.') {
                continue;
            }
            let child_sub = format!("{sub_rel}/{name}");
            let child_ws_rel = format!("{root_rel}/{child_sub}");
            let child_path = self.resolve(&child_ws_rel)?;
            if child_path.is_dir() {
                self.collect_subtree_files(root_rel, &child_sub, out)?;
            } else if child_path.is_file() {
                let bytes = fs::read(&child_path).map_err(|e| io(&child_ws_rel, e))?;
                out.insert(child_sub, bytes);
            }
        }
        Ok(())
    }
}

// ---- Validation & TOML helpers ---------------------------------------------------------

fn is_safe_single_filename(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.starts_with('.')
        && !name.contains(['/', '\\', '\0'])
        && !name.chars().any(char::is_control)
}

fn is_allowed_problem_file(rel: &str) -> bool {
    if rel.is_empty() || rel.contains(['\\', '\0']) || rel.starts_with('/') || rel.ends_with('/') {
        return false;
    }
    let parts: Vec<&str> = rel.split('/').collect();
    match parts.as_slice() {
        [file] => ALLOWED_PROBLEM_ROOT_FILES.contains(file),
        [dir, file] => ALLOWED_PROBLEM_SUBDIRS.contains(dir) && is_safe_single_filename(file),
        _ => false,
    }
}

fn pick_hash_check_target(files: &BTreeMap<String, String>) -> Option<&str> {
    for candidate in [
        "statement.md",
        "statement.zh.md",
        "statement.en.md",
        "editorial.md",
        "problem.toml",
    ] {
        if files.contains_key(candidate) {
            return Some(candidate);
        }
    }
    files.keys().next().map(String::as_str)
}

fn is_snapshot_root_file(name: &str) -> bool {
    name == "problem.toml"
        || name == "editorial.md"
        || name == "statement.md"
        || (name.starts_with("statement.") && name.ends_with(".md"))
}

fn classify_snapshot_file(rel: &str) -> Option<DiffKind> {
    if rel == "problem.toml" {
        Some(DiffKind::MetaChanged)
    } else if rel == "editorial.md" {
        Some(DiffKind::EditorialChanged)
    } else if rel == "statement.md" || (rel.starts_with("statement.") && rel.ends_with(".md")) {
        Some(DiffKind::StatementChanged)
    } else if rel.starts_with("samples/") {
        Some(DiffKind::SamplesChanged)
    } else if rel.starts_with("assets/") {
        Some(DiffKind::AssetsChanged)
    } else {
        None
    }
}

fn parse_doc(path: &str, text: &str) -> Result<DocumentMut> {
    text.parse::<DocumentMut>().map_err(|e| Error::Invalid {
        path: path.to_owned(),
        message: e.to_string().trim_end().to_owned(),
    })
}

/// Replaces a scalar/value entry while preserving its position and surrounding decor
/// (including inline trailing comments).
fn set_scalar(table: &mut dyn TableLike, key: &str, mut val: Value) {
    if let Some(existing) = table.get_mut(key)
        && let Some(old) = existing.as_value()
    {
        *val.decor_mut() = old.decor().clone();
        *existing = Item::Value(val);
        return;
    }
    table.insert(key, toml_edit::value(val));
}

fn normalize_limit_override(patch: &Patch<String>, library_val: Option<&str>) -> Patch<String> {
    match patch {
        Patch::Keep => Patch::Keep,
        Patch::Clear => Patch::Clear,
        Patch::Set(v) => {
            if library_val.unwrap_or("") == v.as_str() {
                Patch::Clear
            } else {
                Patch::Set(v.clone())
            }
        }
    }
}

fn apply_limits_patch(parent: &mut dyn TableLike, patch: &LimitsPatch) {
    let fields = [
        ("time", &patch.time),
        ("memory", &patch.memory),
        ("input", &patch.input),
        ("output", &patch.output),
    ];
    let has_set = fields.iter().any(|(_, p)| matches!(p, Patch::Set(_)));
    if parent.get("limits").is_none() {
        if !has_set {
            return;
        }
        let mut tbl = Table::new();
        tbl.set_implicit(false);
        parent.insert("limits", Item::Table(tbl));
    }

    if let Some(limits_tbl) = parent.get_mut("limits").and_then(Item::as_table_like_mut) {
        for (key, field_patch) in fields {
            match field_patch {
                Patch::Keep => {}
                Patch::Set(v) => set_scalar(limits_tbl, key, Value::from(v.as_str())),
                Patch::Clear => {
                    limits_tbl.remove(key);
                }
            }
        }
    }

    if parent
        .get("limits")
        .and_then(Item::as_table_like)
        .is_some_and(TableLike::is_empty)
    {
        parent.remove("limits");
    }
}

fn apply_sources(parent: &mut dyn TableLike, sources: &[Source]) {
    if sources.is_empty() {
        parent.remove("source");
        return;
    }
    let mut aot = ArrayOfTables::new();
    for src in sources {
        let mut tbl = Table::new();
        tbl["platform"] = toml_edit::value(src.platform.as_str());
        tbl["id"] = toml_edit::value(src.id.as_str());
        if let Some(url) = &src.url {
            tbl["url"] = toml_edit::value(url.as_str());
        }
        aot.push(tbl);
    }
    if let Some(slot) = parent.get_mut("source") {
        *slot = Item::ArrayOfTables(aot);
    } else {
        parent.insert("source", Item::ArrayOfTables(aot));
    }
}

fn apply_contest_meta_patch(root: &mut dyn TableLike, patch: &ContestMetaPatch) -> Result<()> {
    if let Some(name) = &patch.name {
        set_scalar(root, "name", Value::from(name.as_str()));
    }
    match &patch.location {
        Patch::Keep => {}
        Patch::Set(loc) => set_scalar(root, "location", Value::from(loc.as_str())),
        Patch::Clear => {
            root.remove("location");
        }
    }
    match &patch.date {
        Patch::Keep => {}
        Patch::Set(date) => set_scalar(root, "date", Value::from(date.as_str())),
        Patch::Clear => {
            root.remove("date");
        }
    }
    match &patch.author {
        Patch::Keep => {}
        Patch::Set(author) => set_scalar(root, "author", Value::from(author.as_str())),
        Patch::Clear => {
            root.remove("author");
        }
    }
    if let Some(lang) = patch.language {
        set_scalar(root, "language", Value::from(lang.name()));
    }
    if let Some(show) = patch.show_metadata {
        set_scalar(root, "show-metadata", Value::from(show));
    }
    match &patch.cover {
        Patch::Keep => {}
        Patch::Set(cover) => set_scalar(root, "cover", Value::from(*cover)),
        Patch::Clear => {
            root.remove("cover");
        }
    }
    if let Some(blank) = patch.blank_pages {
        set_scalar(root, "blank-pages", Value::from(blank));
    }
    match &patch.template {
        Patch::Keep => {}
        Patch::Set(t) => set_scalar(root, "template", Value::from(t.as_str())),
        Patch::Clear => {
            root.remove("template");
        }
    }
    if !patch.template_options.is_empty() {
        apply_template_options_patch(root, "template-options", &patch.template_options, None)?;
    }
    Ok(())
}

fn option_value_to_toml(val: &TemplateOptionValue) -> Value {
    match val {
        TemplateOptionValue::Bool(b) => Value::from(*b),
        TemplateOptionValue::Int(i) => Value::from(*i),
        TemplateOptionValue::Float(f) => Value::from(*f),
        TemplateOptionValue::String(s) => Value::from(s.as_str()),
        TemplateOptionValue::Lines(lines) => {
            let mut arr = Array::new();
            for line in lines {
                arr.push(line.as_str());
            }
            Value::Array(arr)
        }
    }
}

fn apply_template_options_patch(
    parent: &mut dyn TableLike,
    section_key: &str,
    patches: &BTreeMap<String, TemplateOptionsPatch>,
    base_values: Option<&TemplateOptionsMap>,
) -> Result<()> {
    for (tpl, fields) in patches {
        check_id(tpl)?;
        let has_set = fields.values().any(|p| matches!(p, Patch::Set(_)));
        if parent.get(section_key).is_none() {
            if !has_set {
                continue;
            }
            let mut outer = Table::new();
            outer.set_implicit(true);
            parent.insert(section_key, Item::Table(outer));
        }

        if let Some(outer_tbl) = parent
            .get_mut(section_key)
            .and_then(Item::as_table_like_mut)
        {
            if outer_tbl.get(tpl).is_none() {
                if !has_set {
                    continue;
                }
                let mut inner = Table::new();
                inner.set_implicit(false);
                outer_tbl.insert(tpl, Item::Table(inner));
            }
            if let Some(inner_tbl) = outer_tbl.get_mut(tpl).and_then(Item::as_table_like_mut) {
                let base_for_tpl = base_values.and_then(|m| m.get(tpl));
                for (key, field_patch) in fields {
                    match field_patch {
                        Patch::Keep => {}
                        Patch::Clear => {
                            inner_tbl.remove(key);
                        }
                        Patch::Set(v) => {
                            if base_for_tpl.and_then(|b| b.get(key)) == Some(v) {
                                inner_tbl.remove(key);
                            } else {
                                set_scalar(inner_tbl, key, option_value_to_toml(v));
                            }
                        }
                    }
                }
            }
            if outer_tbl
                .get(tpl)
                .and_then(Item::as_table_like)
                .is_some_and(TableLike::is_empty)
            {
                outer_tbl.remove(tpl);
            }
        }
    }

    if parent
        .get(section_key)
        .and_then(Item::as_table_like)
        .is_some_and(TableLike::is_empty)
    {
        parent.remove(section_key);
    }
    Ok(())
}

fn rewrite_contest_problem_tables(doc: &mut DocumentMut, problem_ids: &[String]) {
    let mut existing: HashMap<String, Table> = HashMap::new();
    let mut min_problem_pos: Option<usize> = None;
    if let Some(aot) = doc.get("problem").and_then(Item::as_array_of_tables) {
        for tbl in aot.iter() {
            if let Some(pos) = tbl.position() {
                min_problem_pos = Some(min_problem_pos.map_or(pos, |m| m.min(pos)));
            }
            if let Some(pid) = tbl.get("id").and_then(Item::as_str) {
                existing.insert(pid.to_owned(), tbl.clone());
            }
        }
    }

    if problem_ids.is_empty() {
        doc.remove("problem");
        return;
    }

    let base_pos = min_problem_pos.unwrap_or_else(|| max_table_position(doc.as_table()) + 1);

    let mut new_aot = ArrayOfTables::new();
    for (idx, pid) in problem_ids.iter().enumerate() {
        let label = label_for_index(idx).expect("problem_ids length validated <= 26");
        let mut tbl = if let Some(mut old) = existing.remove(pid) {
            set_scalar(&mut old, "label", Value::from(label.as_str()));
            old
        } else {
            let mut fresh = Table::new();
            fresh["id"] = toml_edit::value(pid.as_str());
            fresh["label"] = toml_edit::value(label.as_str());
            fresh
        };
        set_position_recursive(&mut tbl, base_pos);
        new_aot.push(tbl);
    }

    if let Some(slot) = doc.get_mut("problem") {
        *slot = Item::ArrayOfTables(new_aot);
    } else {
        doc.insert("problem", Item::ArrayOfTables(new_aot));
    }
}

fn max_table_position(table: &Table) -> usize {
    let mut max_pos = table.position().unwrap_or(0);
    for (_, item) in table.iter() {
        match item {
            Item::Table(sub) => max_pos = max_pos.max(max_table_position(sub)),
            Item::ArrayOfTables(aot) => {
                for sub in aot.iter() {
                    max_pos = max_pos.max(max_table_position(sub));
                }
            }
            _ => {}
        }
    }
    max_pos
}

fn set_position_recursive(table: &mut Table, pos: usize) {
    table.set_position(pos);
    for (_, item) in table.iter_mut() {
        match item {
            Item::Table(sub) => set_position_recursive(sub, pos),
            Item::ArrayOfTables(aot) => {
                for sub in aot.iter_mut() {
                    set_position_recursive(sub, pos);
                }
            }
            _ => {}
        }
    }
}

fn apply_item_overrides_to_toml(raw: &str, item: &ContestItem, path: &str) -> Result<String> {
    if item.title.is_none() && item.limits.is_none() && item.options.is_empty() {
        return Ok(raw.to_owned());
    }
    let mut doc = parse_doc(path, raw)?;
    if let Some(title) = &item.title {
        set_scalar(doc.as_table_mut(), "title", Value::from(title.as_str()));
    }
    if let Some(limits) = &item.limits {
        let patch = LimitsPatch {
            time: limits.time.clone().map_or(Patch::Keep, Patch::Set),
            memory: limits.memory.clone().map_or(Patch::Keep, Patch::Set),
            input: limits.input.clone().map_or(Patch::Keep, Patch::Set),
            output: limits.output.clone().map_or(Patch::Keep, Patch::Set),
        };
        apply_limits_patch(doc.as_table_mut(), &patch);
    }
    if !item.options.is_empty() {
        let patches: BTreeMap<String, TemplateOptionsPatch> = item
            .options
            .iter()
            .map(|(tpl, opts)| {
                let fp = opts
                    .iter()
                    .map(|(k, v)| (k.clone(), Patch::Set(v.clone())))
                    .collect();
                (tpl.clone(), fp)
            })
            .collect();
        apply_template_options_patch(doc.as_table_mut(), "template-options", &patches, None)?;
    }
    Ok(doc.to_string())
}

/// Recursively copies `src_dir` into `dst_dir`, rejecting any symlink anywhere in the tree
/// (`Error::PathEscape`) and verifying every path stays within the workspace.
fn copy_dir_no_symlinks(
    ws: &Workspace,
    src_rel: &str,
    src_dir: &std::path::Path,
    dst_dir: &std::path::Path,
) -> Result<()> {
    fs::create_dir_all(dst_dir).map_err(|e| io(src_rel, e))?;
    let entries = fs::read_dir(src_dir).map_err(|e| io(src_rel, e))?;
    for entry in entries {
        let entry = entry.map_err(|e| io(src_rel, e))?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        let child_rel = format!("{src_rel}/{name_str}");
        let child_src = entry.path();
        let meta = fs::symlink_metadata(&child_src).map_err(|e| io(&child_rel, e))?;
        if meta.file_type().is_symlink() {
            return Err(Error::PathEscape(child_rel));
        }
        ws.resolve(&child_rel)?;
        let child_dst = dst_dir.join(&name);
        if meta.is_dir() {
            copy_dir_no_symlinks(ws, &child_rel, &child_src, &child_dst)?;
        } else if meta.is_file() {
            let bytes = fs::read(&child_src).map_err(|e| io(&child_rel, e))?;
            write_atomic(&child_dst, &bytes).map_err(|e| io(&child_rel, e))?;
        }
    }
    Ok(())
}
