fn replace_backslashes_with_slashes(path: &str) -> String {
    path.replace("\\", "/")
}

pub fn get_project_root() -> String {
    replace_backslashes_with_slashes(env!("PROJECT_ROOT")).to_string()
}

pub fn full_path_from_relative(relative_path: &str) -> String {
    format!("{}{}", get_project_root(), relative_path)
}
