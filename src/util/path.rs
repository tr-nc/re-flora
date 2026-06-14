pub fn get_project_root() -> String {
    let path = verdarium_vkn::project_root();
    let mut root = path.to_string_lossy().replace('\\', "/");
    if !root.ends_with('/') {
        root.push('/');
    }
    root
}
