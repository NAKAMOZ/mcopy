fn main() {
    #[cfg(windows)]
    {
        embed_resource::compile("mcopy.rc", embed_resource::NONE);
    }
}
