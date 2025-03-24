use std::path::Path;

fn replace_backslashes_with_slashes(path: &str) -> String {
    path.replace("\\", "/")
}

pub fn get_project_root() -> String {
    replace_backslashes_with_slashes(env!("PROJECT_ROOT")).to_string()
}

pub fn full_path_from_relative(relative_path: &str) -> String {
    format!("{}{}", get_project_root(), relative_path)
}

pub fn get_full_path_to_dir(full_path_to_file: &str) -> String {
    let path = Path::new(full_path_to_file);
    let parent = path.parent().unwrap();
    let extracted_dir = parent.to_str().unwrap().to_string();
    extracted_dir + "/"
}
