use std::error::Error;
use std::io;

fn main() -> Result<(), Box<dyn Error>> {
    let input = io::read_to_string(io::stdin())?;
    let patch_files = split_diff(input);
    println!("{:#?}", patch_files);
    Ok(())
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
fn split_diff(diff: String) -> Result<Vec<PatchFile>, &'static str>  {
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
            old_file_name = fix_filename(line[4..].to_owned());
        } else if line.starts_with("+++ ") {
            new_file_name = fix_filename(line[4..].to_owned());
        }

        current_file_lines.push(line);
    }

    if old_file_name.is_empty() || new_file_name.is_empty() {
        return Err("Did not find any lines starting with --- or +++ in the diff");
    }

    patch_files.push(PatchFile {
        old: old_file_name.clone(),
        new: new_file_name.clone(),
        contents: current_file_lines.join("\n"),

    });

    Ok(patch_files)

}

fn fix_filename(filename: String) -> String {
    if filename.starts_with("a/") || filename.starts_with("b/") {
        return filename[2..].to_owned();
    }
    return filename;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn split_diff_git() {
        let diff = fs::read_to_string("tests/fixtures/git-multi-file.diff").unwrap();
        let patch_files = split_diff(diff).unwrap();
        assert_eq!(patch_files.len(), 3, "{:#?} does not have length 3", patch_files);
        let first = &patch_files[0];
        assert_eq!(first.old, "Cargo.toml");
        assert_eq!(first.new, "Cargo.toml");
        assert_eq!(first.contents.len(), 279, "Contents: {}", patch_files[0].contents);
        assert!(first.contents.contains("[[bin]]\n"));
    }

}