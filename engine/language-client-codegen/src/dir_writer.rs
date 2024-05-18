use anyhow::Result;
use internal_baml_core::ir::repr::IntermediateRepr;
use std::io::ErrorKind;
use std::path::Path;
use std::thread::sleep;
use std::time::Duration;
use std::{collections::HashMap, path::PathBuf};

// Add a trait per language that can be used to convert an Import into a string
pub(super) trait LanguageFeatures {
    const CONTENT_PREFIX: &'static str;

    fn content_prefix(&self) -> &'static str {
        Self::CONTENT_PREFIX.trim()
    }
}

pub(super) struct FileCollector<L: LanguageFeatures + Default> {
    // map of path to a an object that has the trail File
    files: HashMap<PathBuf, String>,

    lang: L,
}

fn try_delete_tmp_dir(temp_path: &Path) -> Result<()> {
    // if the .tmp dir exists, delete it so we can get back to a working state without user intervention.
    let delete_attempts = 3; // Number of attempts to delete the directory
    let attempt_interval = Duration::from_millis(200); // Wait time between attempts

    for attempt in 1..=delete_attempts {
        if temp_path.exists() {
            match std::fs::remove_dir_all(&temp_path) {
                Ok(_) => {
                    println!("Temp directory successfully removed.");
                    break; // Exit loop after successful deletion
                }
                Err(e) if e.kind() == ErrorKind::Other && attempt < delete_attempts => {
                    log::warn!(
                        "Attempt {}: Failed to delete temp directory: {}",
                        attempt,
                        e
                    );
                    sleep(attempt_interval); // Wait before retrying
                }
                Err(e) => {
                    // For other errors or if it's the last attempt, fail with an error
                    return Err(anyhow::Error::new(e).context(format!(
                        "Failed to delete temp directory '{:?}' after {} attempts",
                        temp_path, attempt
                    )));
                }
            }
        } else {
            break;
        }
    }

    if temp_path.exists() {
        // If the directory still exists after the loop, return an error
        anyhow::bail!(
            "Failed to delete existing temp directory '{:?}' within the timeout",
            temp_path
        );
    }
    Ok(())
}

impl<L: LanguageFeatures + Default> FileCollector<L> {
    pub(super) fn new() -> Self {
        Self {
            files: HashMap::new(),
            lang: L::default(),
        }
    }

    pub(super) fn add_template<
        'ir,
        V: TryFrom<&'ir IntermediateRepr, Error = anyhow::Error> + askama::Template,
    >(
        &mut self,
        name: impl Into<PathBuf> + std::fmt::Display,
        ir: &'ir IntermediateRepr,
    ) -> Result<()> {
        let rendered = V::try_from(ir)
            .map_err(|e| e.context(format!("Error while building {}", name)))?
            .render()
            .map_err(|e| {
                anyhow::Error::from(e).context(format!("Error while rendering {}", name))
            })?;
        self.files.insert(
            name.into(),
            format!("{}\n{}", self.lang.content_prefix(), rendered),
        );
        Ok(())
    }

    pub(super) fn add_file<K: AsRef<str>, V: AsRef<str>>(&mut self, name: K, contents: V) {
        self.files.insert(
            PathBuf::from(name.as_ref()),
            format!("{}\n{}", self.lang.content_prefix(), contents.as_ref()),
        );
    }

    /// Ensure that a directory contains only files we generated before nuking it.
    ///
    /// This is a safety measure to prevent accidentally deleting user files.
    ///
    /// We consider a file to be "generated by BAML" if it contains "generated by BAML"
    /// in the first 1024 bytes, and limit our search to a max of N unrecognized files.
    /// This gives us performance bounds if, for example, we find ourselves iterating
    /// through node_modules or .pycache or some other thing.
    fn remove_dir_safe(&self, output_path: &Path) -> Result<()> {
        if !output_path.exists() {
            return Ok(());
        }

        const MAX_UNKNOWN_FILES: usize = 4;
        let mut unknown_files = vec![];
        for entry in walkdir::WalkDir::new(output_path)
            .into_iter()
            .filter_entry(|e| e.path().file_name().is_some_and(|f| f != "__pycache__"))
        {
            if unknown_files.len() > MAX_UNKNOWN_FILES {
                break;
            }
            let entry = entry?;
            if entry.file_type().is_dir() {
                // Only files matter for the pre-existence check
                continue;
            }
            let path = entry.path();
            if let Ok(mut f) = std::fs::File::open(&path) {
                use std::io::Read;
                let mut buf = [0; 1024];
                if f.read(&mut buf).is_ok()
                    && String::from_utf8_lossy(&buf).contains("generated by BAML")
                {
                    continue;
                }
            }
            let path = path.strip_prefix(output_path)?.to_path_buf();
            unknown_files.push(path);
        }
        unknown_files.sort();
        match unknown_files.len() {
            0 => (),
            1 => anyhow::bail!(
                "output directory contains a file that BAML did not generate\n\n\
                Please remove it and re-run codegen.\n\n\
                File: {}",
                output_path.join(&unknown_files[0]).display()
            ),
            n => {
                if n < MAX_UNKNOWN_FILES {
                    anyhow::bail!(
                        "output directory contains {n} files that BAML did not generate\n\n\
                    Please remove them and re-run codegen.\n\n\
                    Files:\n{}",
                        unknown_files
                            .iter()
                            .map(|p| format!("  - {}", output_path.join(p).display()))
                            .collect::<Vec<_>>()
                            .join("\n")
                    )
                } else {
                    anyhow::bail!(
                        "output directory contains at least {n} files that BAML did not generate\n\n\
                    Please remove all files not generated by BAML and re-run codegen.\n\n\
                    Files:\n{}",
                        unknown_files
                            .iter()
                            .map(|p| format!("  - {}", output_path.join(p).display()))
                            .collect::<Vec<_>>()
                            .join("\n")
                    )
                }
            }
        }
        std::fs::remove_dir_all(output_path)?;
        Ok(())
    }

    pub(super) fn commit(&self, output_path: &Path) -> Result<Vec<PathBuf>> {
        log::debug!("Writing files to {}", output_path.display());

        let temp_path = PathBuf::from(format!("{}.tmp", output_path.display()));

        // if the .tmp dir exists, delete it so we can get back to a working state without user intervention.
        try_delete_tmp_dir(temp_path.as_path())?;

        // Sort the files by path so that we always write to the same file
        for (relative_file_path, contents) in self.files.iter() {
            let full_file_path = temp_path.join(relative_file_path);
            if let Some(parent) = full_file_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&full_file_path, contents)?;
        }

        self.remove_dir_safe(output_path)?;
        std::fs::rename(&temp_path, output_path)?;

        log::info!(
            "Wrote {} files to {}",
            self.files.len(),
            output_path.display()
        );

        Ok(self.files.keys().cloned().collect())
    }
}
