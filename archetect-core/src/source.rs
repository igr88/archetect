use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;

use log::{debug, info};
use regex::Regex;
use url::Url;

use crate::requirements::{Requirements, RequirementsError};
use crate::Archetect;

#[derive(Clone, Debug, PartialOrd, PartialEq)]
pub enum Source {
    RemoteGit { url: String, path: PathBuf, gitref: Option<String> },
    RemoteHttp { url: String, path: PathBuf },
    LocalDirectory { path: PathBuf },
    LocalFile { path: PathBuf },
}

#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    #[error("Unsupported source: `{0}`")]
    SourceUnsupported(String),
    #[error("Failed to find a default 'develop', 'main', or 'master' branch.")]
    NoDefaultBranch,
    #[error("Source not found: `{0}`")]
    SourceNotFound(String),
    #[error("Invalid Source Path: `{0}`")]
    SourceInvalidPath(String),
    #[error("Invalid Source Encoding: `{0}`")]
    SourceInvalidEncoding(String),
    #[error("Remote Source Error: `{0}`")]
    RemoteSourceError(String),
    #[error("Remote Source is not cached, and Archetect was run in offline mode: `{0}`")]
    OfflineAndNotCached(String),
    #[error("Source IO Error: `{0}`")]
    IoError(std::io::Error),
    #[error("Requirements Error in `{path}`: {cause}")]
    RequirementsError { path: String, cause: RequirementsError },
}

impl From<std::io::Error> for SourceError {
    fn from(error: std::io::Error) -> SourceError {
        SourceError::IoError(error)
    }
}

lazy_static! {
    static ref SSH_GIT_PATTERN: Regex = Regex::new(r"\S+@(\S+):(.*)").unwrap();
    static ref CACHED_PATHS: Mutex<HashSet<String>> = Mutex::new(HashSet::new());
}

impl Source {
    pub fn detect(archetect: &Archetect, path: &str, relative_to: Option<Source>) -> Result<Source, SourceError> {
        let source = path;
        let git_cache = archetect.layout().git_cache_dir();

        let urlparts: Vec<&str> = path.split('#').collect();
        if let Some(captures) = SSH_GIT_PATTERN.captures(&urlparts[0]) {

            let cache_path = git_cache
                .clone()
                .join(get_cache_key(format!("{}/{}", &captures[1], &captures[2])));

            let gitref = if urlparts.len() > 1 { Some(urlparts[1].to_owned()) } else { None };
            if let Err(error) = cache_git_repo(urlparts[0], &gitref, &cache_path, archetect
                .offline()) {
                return Err(error);
            }
            verify_requirements(archetect, source, &cache_path)?;
            return Ok(Source::RemoteGit {
                url: path.to_owned(),
                path: cache_path,
                gitref,
            });
        };

        if let Ok(url) = Url::parse(&path) {
            if path.contains(".git") && url.has_host() {
                let cache_path =
                    git_cache
                        .clone()
                        .join(get_cache_key(format!("{}/{}", url.host_str().unwrap(), url.path())));
                let gitref = url.fragment().map_or(None, |r| Some(r.to_owned()));
                if let Err(error) = cache_git_repo(urlparts[0], &gitref, &cache_path, archetect.offline()) {
                    return Err(error);
                }
                verify_requirements(archetect, source, &cache_path)?;
                return Ok(Source::RemoteGit {
                    url: path.to_owned(),
                    path: cache_path,
                    gitref,
                });
            }

            if let Ok(local_path) = url.to_file_path() {
                return if local_path.exists() {
                    verify_requirements(archetect, source, &local_path)?;
                    Ok(Source::LocalDirectory { path: local_path })
                } else {
                    Err(SourceError::SourceNotFound(local_path.display().to_string()))
                };
            }
        }

        if let Ok(path) = shellexpand::full(&path) {
            let local_path = PathBuf::from(path.as_ref());
            if local_path.is_relative() {
                if let Some(parent) = relative_to {
                    let local_path = parent.local_path().clone().join(local_path);
                    if local_path.exists() && local_path.is_dir() {
                        verify_requirements(archetect, source, &local_path)?;
                        return Ok(Source::LocalDirectory { path: local_path });
                    } else {
                        return Err(SourceError::SourceNotFound(local_path.display().to_string()));
                    }
                }
            }
            if local_path.exists() {
                if local_path.is_dir() {
                    verify_requirements(archetect, source, &local_path)?;
                    return Ok(Source::LocalDirectory { path: local_path });
                } else {
                    return Ok(Source::LocalFile { path: local_path });
                }
            } else {
                return Err(SourceError::SourceNotFound(local_path.display().to_string()));
            }
        } else {
            return Err(SourceError::SourceInvalidPath(path.to_string()));
        }
    }

    pub fn directory(&self) -> &Path {
        match self {
            Source::RemoteGit { url: _, path, gitref: _ } => path.as_path(),
            Source::RemoteHttp { url: _, path } => path.as_path(),
            Source::LocalDirectory { path } => path.as_path(),
            Source::LocalFile { path } => path.parent().unwrap_or(path),
        }
    }

    pub fn local_path(&self) -> &Path {
        match self {
            Source::RemoteGit { url: _, path, gitref: _ } => path.as_path(),
            Source::RemoteHttp { url: _, path } => path.as_path(),
            Source::LocalDirectory { path } => path.as_path(),
            Source::LocalFile { path } => path.as_path(),
        }
    }

    pub fn source(&self) -> &str {
        match self {
            Source::RemoteGit { url, path: _, gitref: _ } => url,
            Source::RemoteHttp { url, path: _ } => url,
            Source::LocalDirectory { path } => path.to_str().unwrap(),
            Source::LocalFile { path } => path.to_str().unwrap(),
        }
    }
}

fn get_cache_hash<S: AsRef<[u8]>>(input: S) -> u64 {
    let result = farmhash::fingerprint64(input.as_ref());
    result
}

fn get_cache_key<S: AsRef<[u8]>>(input: S) -> String {
    format!("{}", get_cache_hash(input))
}

fn verify_requirements(archetect: &Archetect, source: &str, path: &Path) -> Result<(), SourceError> {
    match Requirements::load(&path) {
        Ok(results) => {
            if let Some(requirements) = results {
                if let Err(error) = requirements.verify(archetect) {
                    return Err(SourceError::RequirementsError {
                        path: source.to_owned(),
                        cause: error,
                    });
                }
            }
        }
        Err(error) => {
            return Err(SourceError::RequirementsError {
                path: path.display().to_string(),
                cause: error,
            });
        }
    }
    Ok(())
}

fn cache_git_repo(url: &str, gitref: &Option<String>, cache_destination: &Path, offline: bool) -> Result<(),
    SourceError> {
    if !cache_destination.exists() {
        if !offline && CACHED_PATHS.lock().unwrap().insert(url.to_owned()) {
            info!("Cloning {}", url);
            debug!("Cloning to {}", cache_destination.to_str().unwrap());
            handle_git(Command::new("git").args(&["clone", &url, cache_destination.to_str().unwrap()]))?;
        } else {
            return Err(SourceError::OfflineAndNotCached(url.to_owned()));
        }
    } else {
        if !offline && CACHED_PATHS.lock().unwrap().insert(url.to_owned()) {
            info!("Fetching {}", url);
            handle_git(Command::new("git").current_dir(&cache_destination).args(&["fetch"]))?;
        }
    }

    let gitref = if let Some(gitref) = gitref {
        gitref.to_owned()
    } else {
        find_default_branch(&cache_destination.to_str().unwrap())?
    };

    let gitref_spec = if is_branch(&cache_destination.to_str().unwrap(), &gitref) {
        format!("origin/{}", &gitref)
    } else {
        gitref
    };

    debug!("Checking out {}", gitref_spec);
    handle_git(Command::new("git").current_dir(&cache_destination).args(&["checkout", &gitref_spec]))?;

    Ok(())
}

fn is_branch(path: &str, gitref: &str) -> bool {
    match handle_git(Command::new("git").current_dir(path)
        .arg("show-ref")
        .arg("-q")
        .arg("--verify")
        .arg(format!("refs/remotes/origin/{}", gitref))) {
        Ok(_) => true,
        Err(_) => false,
    }
}

fn find_default_branch(path: &str) -> Result<String, SourceError> {
    for candidate in &["develop", "main", "master"] {
        if is_branch(path, candidate) {
            return Ok((*candidate).to_owned());
        }
    }
    Err(SourceError::NoDefaultBranch)
}

fn handle_git(command: &mut Command) -> Result<(), SourceError> {
    if cfg!(target_os = "windows") {
        command.stdin(Stdio::inherit());
        command.stderr(Stdio::inherit());
    }
    match command.output() {
        Ok(output) => match output.status.code() {
            Some(0) => Ok(()),
            Some(error_code) => Err(SourceError::RemoteSourceError(format!(
                "Error Code: {}\n{}",
                error_code,
                String::from_utf8(output.stderr)
                    .unwrap_or("Error reading error code from failed git command".to_owned())
            ))),
            None => Err(SourceError::RemoteSourceError("Git interrupted by signal".to_owned())),
        },
        Err(err) => Err(SourceError::IoError(err)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_hash() {
        println!(
            "{}",
            get_cache_hash("https://raw.githubusercontent.com/archetect/archetect/master/LICENSE-MIT-MIT")
        );
        println!(
            "{}",
            get_cache_hash("https://raw.githubusercontent.com/archetect/archetect/master/LICENSE-MIT-MIT")
        );
        println!("{}", get_cache_hash("f"));
        println!("{}", get_cache_hash("1"));
    }

    #[test]
    fn test_http_source() {
        let archetect = Archetect::build().unwrap();
        let source = Source::detect(
            &archetect,
            "https://raw.githubusercontent.com/archetect/archetect/master/LICENSE-MIT-MIT",
            None,
        );
        println!("{:?}", source);
    }

    //    use super::*;
    //    use matches::assert_matches;

    //    #[test]
    //    fn test_detect_short_git_url() {
    //        // TODO: Fix this test.
    //        assert_matches!(
    //            Location::detect("git@github.com:jimmiebfulton/archetect.git", ),
    //            Ok(Location::RemoteGit { url: _, path: _ })
    //        );
    //    }
    //
    //    #[test]
    //    fn test_detect_http_git_url() {
    //        // TODO: Fix this test.
    //        assert_matches!(
    //            Location::detect("https://github.com/jimmiebfulton/archetect.git"),
    //            Ok(Location::RemoteGit { url: _, path: _ })
    //        );
    //    }
    //
    //    #[test]
    //    fn test_detect_local_directory() {
    //        assert_eq!(
    //            Location::detect(".", false),
    //            Ok(Location::LocalDirectory {
    //                path: PathBuf::from(".")
    //            })
    //        );
    //
    //        assert_matches!(
    //            Location::detect("~"),
    //            Ok(Location::LocalDirectory { path: _ })
    //        );
    //
    //        assert_eq!(
    //            Location::detect("notfound", false),
    //            Err(LocationError::LocationNotFound)
    //        );
    //    }
    //
    //    #[test]
    //    fn test_file_url() {
    //        assert_eq!(
    //            Location::detect("file://localhost/home", false),
    //            Ok(Location::LocalDirectory {
    //                path: PathBuf::from("/home")
    //            }),
    //        );
    //
    //        assert_eq!(
    //            Location::detect("file:///home", false),
    //            Ok(Location::LocalDirectory {
    //                path: PathBuf::from("/home")
    //            }),
    //        );
    //
    //        assert_eq!(
    //            Location::detect("file://localhost/nope", false),
    //            Err(LocationError::LocationNotFound),
    //        );
    //
    //        assert_eq!(
    //            Location::detect("file://nope/home", false),
    //            Err(LocationError::LocationUnsupported),
    //        );
    //    }
    //
    //    #[test]
    //    fn test_short_git_pattern() {
    //        let captures = SSH_GIT_PATTERN
    //            .captures("git@github.com:jimmiebfulton/archetect.git")
    //            .unwrap();
    //        assert_eq!(&captures[1], "github.com");
    //        assert_eq!(&captures[2], "jimmiebfulton/archetect.git");
    //    }
}
