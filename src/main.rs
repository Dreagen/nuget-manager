use std::{
    collections::HashSet,
    error::Error,
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
    time::Duration,
};

use serde::{Deserialize, Serialize};

type Result<T> = core::result::Result<T, Box<dyn Error>>;

const GREEN: &str = "\x1b[32m";
const RED: &str = "\x1b[31m";
const YELLOW: &str = "\x1b[33m";
const BLUE: &str = "\x1b[34m";
const RESET: &str = "\x1b[0m";

fn main() {
    print!("Loading projects...");
    io::stdout().flush().unwrap();

    find_projects(Path::new("."))
        .into_iter()
        .for_each(|p| p.print());
}

fn find_projects(path: &Path) -> Vec<Project> {
    let Ok(paths) = fs::read_dir(path) else {
        println!("Couldn't read directory: {}", path.to_string_lossy());
        return vec![];
    };

    let mut projects = vec![];

    for path in paths.filter_map(|p| p.ok()) {
        match path.file_type() {
            Ok(file_type) if file_type.is_dir() => {
                projects.append(&mut find_projects(&path.path()))
            }
            Ok(_) if path.file_name().to_string_lossy().ends_with(".csproj") => {
                let name = path.file_name().to_string_lossy().into_owned();
                let spinner = print_with_spinner(format!(
                    "\x1b[2K\rLoading reference information for {}{}{}",
                    GREEN, name, RESET
                ));

                let result = Project::new(path.path(), name);

                spinner.stop();

                match result {
                    Ok(project) => projects.push(project),
                    Err(err) => println!("{}", err),
                }
            }
            Err(e) => println!(
                "couldn't get file type from path: {} - error: {}",
                path.file_name().to_string_lossy(),
                e
            ),
            _ => {}
        }
    }

    projects
}

fn print_with_spinner(spinner_msg: String) -> Spinner {
    let stop_flag = Arc::new(Mutex::new(false));
    let stop_clone = Arc::clone(&stop_flag);
    let spinner = thread::spawn(move || {
        const FRAMES: &[char] = &['⣾', '⣽', '⣻', '⢿', '⡿', '⣟', '⣯', '⣷'];
        let mut i = 0;
        while !*stop_clone.lock().unwrap() {
            print!(
                "{} {}{}{} ",
                spinner_msg,
                YELLOW,
                FRAMES[i % FRAMES.len()],
                RESET
            );
            io::stdout().flush().unwrap();
            thread::sleep(Duration::from_millis(100));
            i += 1;
        }
    });

    Spinner {
        stop_flag,
        handle: spinner,
    }
}

impl Spinner {
    fn stop(self) {
        *self.stop_flag.lock().unwrap() = true;
        self.handle.join().unwrap();
    }
}

struct Spinner {
    stop_flag: Arc<Mutex<bool>>,
    handle: JoinHandle<()>,
}

impl Project {
    fn new(path: PathBuf, name: String) -> Result<Self> {
        let mut package_references = fs::read_to_string(&path)?
            .lines()
            .map(|line| line.to_string())
            .filter(|line| line.trim_start().starts_with("<PackageReference"))
            .map(PackageReference::new)
            .inspect(|package_ref| {
                if package_ref.is_err() {
                    println!("{}", package_ref.as_ref().unwrap_err().to_string());
                }
            })
            .filter_map(|r| r.ok())
            .collect::<Vec<PackageReference>>();

        for package_reference in &mut package_references {
            let mut child = Command::new("dotnet")
                .args([
                    "package",
                    "search",
                    &package_reference.name,
                    "--format",
                    "json",
                    "--exact-match",
                ])
                .stdout(Stdio::piped())
                .spawn()
                .unwrap();

            let mut buffer = String::new();

            child
                .stdout
                .as_mut()
                .unwrap()
                .read_to_string(&mut buffer)
                .unwrap();

            match serde_json::from_str::<VersionInformation>(&buffer) {
                Ok(version_information) => {
                    package_reference.available_versions = version_information
                        .search_result
                        .into_iter()
                        .filter(|r| {
                            r.packages
                                .iter()
                                .any(|p| p.id.eq_ignore_ascii_case(&package_reference.name))
                        })
                        .flat_map(|search_result| {
                            search_result
                                .packages
                                .into_iter()
                                .filter(|p| p.id.eq_ignore_ascii_case(&package_reference.name))
                                .map(|p| PackageVersion::new(p.version))
                        })
                        .collect()
                }
                Err(e) => println!(
                    "Failed to parse version information for project: {} - {}",
                    name, e
                ),
            }

            package_reference.latest_version = package_reference
                .available_versions
                .iter()
                .filter_map(|p| SemanticPackageVersion::from_package_version(p))
                .max()
                .map(|s| s.to_package_version());

            if package_reference.latest_version.is_none() {
                package_reference.status = PackageReferenceStatus::Unknown;
            }

            let latest = &package_reference.latest_version;
            package_reference.status = match SemanticPackageVersion::from_package_version(
                &package_reference.current_version,
            ) {
                Some(current_version) => current_version.compare(
                    &SemanticPackageVersion::from_package_version(&latest.as_ref().unwrap()),
                ),
                None => PackageReferenceStatus::Unknown,
            }
        }

        Ok(Project {
            name,
            references: package_references,
        })
    }

    fn print(&self) {
        if self.references.len() == 0 {
            return;
        }

        println!("\n{}", self.name);
        println!(
            "{:<5} {:<70} {:<15} {:<15}",
            "", "Name", "Current", "Latest"
        );

        for reference in &self.references {
            let latest = reference
                .get_latest_available_version()
                .map(|v| v.version)
                .unwrap_or_default();

            let colour = match reference.status {
                PackageReferenceStatus::Unknown => RESET,
                PackageReferenceStatus::UpToDate => GREEN,
                PackageReferenceStatus::BehindMajor => RED,
                PackageReferenceStatus::BehindMinor => YELLOW,
                PackageReferenceStatus::BehindPatch => BLUE,
            };

            println!(
                "{:<5} {:<70} {}{:<15}{} {:<15}",
                "", reference.name, colour, reference.current_version.version, RESET, latest
            );
        }
    }
}

#[derive(Debug)]
struct Project {
    name: String,
    references: Vec<PackageReference>,
}

impl PackageReference {
    fn new(package_reference_line: String) -> Result<Self> {
        let name_start = package_reference_line
            .find("Include=\"")
            .ok_or("no Include=\" found in package reference line")?
            + "Include=\"".len();

        let name_end = package_reference_line[name_start..]
            .find('"')
            .ok_or("no trailing \" after Include=\" found in package reference line")?
            + name_start;

        let version_start = package_reference_line
            .find("Version=\"")
            .ok_or("no Version=\" found in package reference line")?
            + "Version=\"".len();

        let version_end = package_reference_line[version_start..]
            .find('"')
            .ok_or("no trailing \" after Version=\" found in package reference line")?
            + version_start;

        Ok(PackageReference {
            name: package_reference_line[name_start..name_end].to_string(),
            current_version: PackageVersion {
                version: package_reference_line[version_start..version_end].to_string(),
            },
            latest_version: None,
            status: PackageReferenceStatus::Unknown,
            available_versions: HashSet::new(),
        })
    }

    fn get_latest_available_version(&self) -> Option<PackageVersion> {
        self.available_versions
            .iter()
            .filter_map(|p| SemanticPackageVersion::from_package_version(p))
            .max()
            .map(|s| s.to_package_version())
    }
}

#[derive(Debug)]
struct PackageReference {
    name: String,
    current_version: PackageVersion,
    latest_version: Option<PackageVersion>,
    available_versions: HashSet<PackageVersion>,
    status: PackageReferenceStatus,
}

#[derive(Debug)]
enum PackageReferenceStatus {
    Unknown,
    UpToDate,
    BehindMajor,
    BehindMinor,
    BehindPatch,
}

impl PackageVersion {
    fn new(version: String) -> Self {
        PackageVersion { version }
    }
}

#[derive(Debug, Hash, PartialEq, Eq)]
struct PackageVersion {
    version: String,
}

impl Ord for SemanticPackageVersion {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        if self.major < other.major {
            return std::cmp::Ordering::Less;
        } else if self.major > other.major {
            return std::cmp::Ordering::Greater;
        } else if self.minor < other.minor {
            return std::cmp::Ordering::Less;
        } else if self.minor > other.minor {
            return std::cmp::Ordering::Greater;
        } else if self.patch < other.patch {
            return std::cmp::Ordering::Less;
        } else if self.patch > other.patch {
            return std::cmp::Ordering::Greater;
        } else {
            return std::cmp::Ordering::Equal;
        }
    }
}

impl SemanticPackageVersion {
    fn from_package_version(package_version: &PackageVersion) -> Option<SemanticPackageVersion> {
        let parts = package_version.version.split('.').collect::<Vec<&str>>();

        if parts.len() != 3 {
            return None;
        }

        let major = isize::from_str_radix(parts[0], 10);
        if major.is_err() {
            return None;
        }

        let minor = isize::from_str_radix(parts[1], 10);
        if minor.is_err() {
            return None;
        }

        let patch = isize::from_str_radix(parts[2], 10);
        if patch.is_err() {
            return None;
        }

        let semantic_package_version = SemanticPackageVersion {
            major: major.unwrap(),
            minor: minor.unwrap(),
            patch: patch.unwrap(),
        };

        Some(semantic_package_version)
    }

    fn to_package_version(&self) -> PackageVersion {
        PackageVersion {
            version: format!("{}.{}.{}", self.major, self.minor, self.patch),
        }
    }

    fn compare(&self, other: &Option<SemanticPackageVersion>) -> PackageReferenceStatus {
        match other {
            Some(other_version) if self.major < other_version.major => {
                PackageReferenceStatus::BehindMajor
            }
            Some(other_version) if self.minor < other_version.minor => {
                PackageReferenceStatus::BehindMinor
            }
            Some(other_version) if self.patch < other_version.patch => {
                PackageReferenceStatus::BehindPatch
            }
            Some(_) => PackageReferenceStatus::UpToDate,
            None => PackageReferenceStatus::Unknown,
        }
    }
}

#[derive(PartialOrd, PartialEq, Eq, Debug)]
struct SemanticPackageVersion {
    major: isize,
    minor: isize,
    patch: isize,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VersionInformation {
    search_result: Vec<VersionSearchResult>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VersionSearchResult {
    source_name: String,
    packages: Vec<VersionPackage>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VersionPackage {
    id: String,
    version: String,
}
