use std::fs;
use std::io;
use std::io::Write; // For buf in logger.
use std::path::{Path, PathBuf};

use anyhow::bail;
use atty;
use env_logger;
use git2::{DiffFormat, DiffOptions, Repository};
use log::{debug, info, LevelFilter};

const DEFAULT_REMOTE_HEAD: &'static str = "refs/remotes/origin/HEAD";
// When printing a diff, we need to prefix certain lines with an extra
// character, if that line indicates it has a certain type of "origin"
// (see DiffLine in git2). These origins are exactly what diff_print_to_buf
// checks against in libgit2, and that function claims it prints identically
// to `git diff`.
const GIT_DIFF_ORIGINS_TO_PRINT: [char; 3] = ['+', '-', ' '];

/*
Define a set of characters we consider unsafe in filenames.
On Windows, for instance, these characters are not allowed in filenames:
< > : " / \ | ? *
We'll also replace the directory separator `/` commonly used on Unix,
plus we replace . because we are adding our own extension.
*/
const FILENAME_FORBIDDEN_CHARS: [char; 10] = ['/', '<', '>', ':', '"', '\\', '|', '?', '*', '.'];

// TODO: Use miette to colorize error output?
fn main() -> anyhow::Result<()> {
    env_logger::Builder::new()
        .filter_level(LevelFilter::Info)
        .format(|buf, record| writeln!(buf, "{}", record.args()))
        .parse_default_env()
        .init();

    let mut input = String::new();
    if atty::isnt(atty::Stream::Stdin) {
        info!("Detected input on stdin, reading a diff from stdin.");
        input = io::read_to_string(io::stdin())?;
    } else {
        match Repository::discover(Path::new(".")) {
            Ok(repo) => {
                info!("Diffing the local git repository against remote head.");
                input = get_diff_from_repo(repo)?;
            }
            Err(e) => {
                debug!("No git repo found: {}", e)
            }
        };
    }
    if input.is_empty() {
        bail!("No input found on stdin, and local directory is not a git repo that differs from remote head.");
    }
    let patch_files = split_diff(input)?;
    write_out_new_diffs(patch_files)?;
    Ok(())
}

fn get_diff_from_repo(repo: Repository) -> Result<String, git2::Error> {
    /*
    We want to find the "merge base commit." Basically, we want to know
    the differences between our repo and what origin would have looked
    like the last time we merged (so that we don't force the user to)
    fetch from head, although we should probably warn them if they
    haven't merged in head (since that will mean we then have to merge)
    on every individual repo we create, later. But maybe somebody is using
    this tool for some other purpose, so we allow this.
    */

    let local_head = repo.head()?.peel_to_commit()?;
    // TODO: Allow user to specify a different remote.
    let remote_head = repo
        .find_reference(&DEFAULT_REMOTE_HEAD)?
        .peel_to_commit()?;
    // TODO: Wrap this error to give it a better error message.
    let merge_base_oid = repo.merge_base(local_head.id(), remote_head.id())?;
    let merge_base_commit = repo.find_commit(merge_base_oid)?;
    let local_head_tree = local_head.tree()?;
    let merge_base_tree = merge_base_commit.tree()?;

    // TODO: Provide an option to choose between diffing against the workdir and
    // diffing against committed head.
    //
    // TODO: Need to support binary files that have changed.
    //
    // We default to using the committed head because we assume that the user's
    // intent is to create diffs against what would be pushed as a PR if they
    // pushed right now.
    let diff = repo.diff_tree_to_tree(
        Some(&merge_base_tree),
        Some(&local_head_tree),
        Some(&mut DiffOptions::new()),
    )?;

    let mut diff_text = String::new();
    diff.print(DiffFormat::Patch, |_delta, _hunk, line| {
        // This algorithm is similar to the one inside libgit2 for printing
        // out exactly like git diff does. (Rust git2 does not expose
        // `diff_print_to_buf` as of Jan 1, 2025.)
        if GIT_DIFF_ORIGINS_TO_PRINT.contains(&line.origin()) {
            diff_text.push(line.origin());
        }
        diff_text.push_str(&String::from_utf8_lossy(line.content()));
        true
    })?;
    Ok(diff_text)
}

#[derive(Debug)]
struct PatchFile {
    old: String,
    new: String,
    contents: String,
}

/*
Originally I was using the patch crate to parse patches, but it does both
less and more than I need. I don't need to understand hunks (which patch
does) but I do want to, ideally, preserve all the header content of each
patch (which patch does not).

This does not borrow the input string, because in all callers we never
need to re-use the string again.
*/
fn split_diff(diff: String) -> anyhow::Result<Vec<PatchFile>> {
    let mut patch_files = Vec::new();
    let mut current_file_lines = Vec::new();
    let mut old_file_name = String::new();
    let mut new_file_name = String::new();

    // TODO: We will need to deal with outputting CRLF correctly, in the future.
    // Although to be fair, I'm not sure that actually matters for most patch
    // tools. Can use diff.split_inclusive('\n').
    for line in diff.lines() {
        /*
        In many patch formats, such as git, this is the indicator that
        we are starting a new file.

        We check if we have ever set old_file_name here, because some diff
        formats have a header at the top before any file info, and we want
        to preserve that as part of the first file. (In the future, maybe
        we preserve it as a separate return value.)
        */
        if line.starts_with("diff ") && !old_file_name.is_empty() {
            patch_files.push(PatchFile {
                old: old_file_name.clone(),
                new: new_file_name.clone(),
                contents: current_file_lines.join("\n"),
            });
            current_file_lines.clear();
            old_file_name.clear();
            new_file_name.clear();
        } else if line.starts_with("--- ") {
            old_file_name = fix_filename_in_diff(line[4..].to_owned());
        } else if line.starts_with("+++ ") {
            new_file_name = fix_filename_in_diff(line[4..].to_owned());
        }

        current_file_lines.push(line);
    }

    if old_file_name.is_empty() || new_file_name.is_empty() {
        bail!("Did not find any lines starting with --- or +++ in the diff");
    }

    patch_files.push(PatchFile {
        old: old_file_name.clone(),
        new: new_file_name.clone(),
        contents: current_file_lines.join("\n"),
    });

    Ok(patch_files)
}

fn fix_filename_in_diff(mut filename: String) -> String {
    // Prefixes used by git diff.
    if filename.starts_with("a/") || filename.starts_with("b/") {
        filename = filename[2..].to_owned();
    }
    // The normal "diff" command adds dates after file paths, delimited
    // with a tab (which is not a valid path character on any OS that I
    // know of).
    if let Some(tab_pos) = filename.find('\t') {
        filename = filename[..tab_pos].to_owned();
    }
    filename
}

fn write_out_new_diffs(patch_files: Vec<PatchFile>) -> Result<(), io::Error> {
    for pf in patch_files {
        let new_path = generate_filename(&pf)?;
        info!("Writing: {}", new_path.to_string_lossy());
        // Theoretically there is a TOCTOU issue here.
        fs::write(new_path, pf.contents)?;
    }
    Ok(())
}

fn generate_filename(pf: &PatchFile) -> Result<PathBuf, io::Error> {
    // By default, we want to use the new filename. However, in some patch
    // formats it's "/dev/null" for deleted files, and we don't just want
    // to write out a bunch of files named _dev_null.
    let mut diff_filename = &pf.new;
    if diff_filename == "/dev/null" {
        diff_filename = &pf.old;
    }
    let base_filename: String = diff_filename
        .chars()
        .map(|c| {
            if FILENAME_FORBIDDEN_CHARS.contains(&c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let mut with_ext = Path::new(&base_filename).with_extension("diff");
    let mut counter = 0;
    // TODO: Add retry limit?
    loop {
        if !with_ext.try_exists()? {
            return Ok(with_ext.to_path_buf());
        }
        counter += 1;
        with_ext.set_file_name(&format!("{}-{}.diff", base_filename, counter));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use googletest::prelude::*;
    use tempfile;

    #[gtest]
    fn generate_filename_simple_filename() {
        let pf = PatchFile {
            old: "foo".to_string(),
            new: "bar".to_string(),
            contents: "nothing".to_string(),
        };
        expect_that!(generate_filename(&pf), ok(eq(Path::new("bar.diff"))));
    }

    #[gtest]
    fn generate_filename_dev_null() {
        let pf = PatchFile {
            old: "foo".to_string(),
            new: "/dev/null".to_string(),
            contents: "nothing".to_string(),
        };
        expect_that!(generate_filename(&pf), ok(eq(Path::new("foo.diff"))));
    }

    #[gtest]
    fn generate_filename_file_with_extension() {
        let pf = PatchFile {
            old: "foo.diff".to_string(),
            new: "bar.diff".to_string(),
            contents: "nothing".to_string(),
        };
        expect_that!(generate_filename(&pf), ok(eq(Path::new("bar_diff.diff"))));
    }

    #[gtest]
    fn generate_filename_file_exists() {
        // We have to use Builder or tempfile will add a . as a prefix.
        let tmp = tempfile::Builder::new()
            .prefix("filename_exists-")
            .suffix(".diff")
            .tempfile_in("./")
            .unwrap();
        let without_ext = tmp
            .path()
            .with_extension("")
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        let pf = PatchFile {
            old: "foo.diff".to_string(),
            new: without_ext.clone(),
            contents: "nothing".to_string(),
        };
        let expect_name = format!("{}-1.diff", without_ext);
        expect_that!(generate_filename(&pf), ok(eq(Path::new(&expect_name))));
    }

    #[gtest]
    fn split_diff_git() {
        let diff = fs::read_to_string("tests/fixtures/git-multi-file.diff").unwrap();
        let patch_files = split_diff(diff).unwrap();
        assert_that!(patch_files, len(eq(3)));
        check_patch_file(
            &patch_files[0],
            "Cargo.toml",
            "Cargo.toml",
            279,
            "[[bin]]\n",
        );
        // TODO: This does not preserve the newline on the last line, currently.
        check_patch_file(&patch_files[1], "src/main.rs", "/dev/null", 181, "-}");
        check_patch_file(
            &patch_files[2],
            "/dev/null",
            "src/splitpr.rs",
            416,
            "+    Ok(())\n",
        );
    }

    // A patch generated with "diff -Nru"
    #[gtest]
    fn split_diff_diff() {
        let diff = fs::read_to_string("tests/fixtures/diff-Nru-multi-file.diff").unwrap();
        let patch_files = split_diff(diff).unwrap();
        assert_that!(patch_files, len(eq(3)));
        check_patch_file(
            &patch_files[0],
            "multipr-2/Cargo.toml",
            "multipr-3/Cargo.toml",
            332,
            "[[bin]]\n",
        );
        // TODO: This does not preserve the newline on the last line, currently.
        //
        // Note that this is an important difference from git diff: there is no /dev/null when you're
        // adding or removing a file. Instead, the added and removed file name are the same but with
        // different base directories, as though there was an empty file in the new or old location.
        check_patch_file(
            &patch_files[1],
            "multipr-2/src/main.rs",
            "multipr-3/src/main.rs",
            240,
            "-}",
        );
        check_patch_file(
            &patch_files[2],
            "multipr-2/src/splitpr.rs",
            "multipr-3/src/splitpr.rs",
            482,
            "+    Ok(())\n",
        );
    }

    fn check_patch_file(
        item: &PatchFile,
        old: &str,
        new: &str,
        expected_length: usize,
        check_contents: &str,
    ) {
        expect_that!(item.old, eq(old));
        expect_that!(item.new, eq(new));
        expect_that!(item.contents, contains_substring(check_contents));
        expect_that!(item.contents, char_count(eq(expected_length)));
    }
}
