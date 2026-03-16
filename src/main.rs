use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
};

type Result<T> = core::result::Result<T, Box<dyn Error>>;

fn main() {
    let projects = find_projects(Path::new("."));

    for project in projects {
        println!("project found: {:?}", project);
    }
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
                match Project::new(path.path(), path.file_name().to_string_lossy().into_owned()) {
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

impl Project {
    fn new(path: PathBuf, name: String) -> Result<Self> {
        let package_references = fs::read_to_string(&path)?
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

        Ok(Project {
            path,
            name,
            references: package_references,
        })
    }
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
            version: PackageVersion {
                version: package_reference_line[version_start..version_end].to_string(),
            },
        })
    }
}

#[derive(Debug)]
struct Project {
    path: PathBuf,
    name: String,
    references: Vec<PackageReference>,
}

#[derive(Debug)]
struct PackageReference {
    name: String,
    version: PackageVersion,
}

#[derive(Debug)]
struct PackageVersion {
    version: String,
}
