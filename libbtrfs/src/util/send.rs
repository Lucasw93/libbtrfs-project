/// Get the supportes, send version for a btrfs filesystem. Default to v1 if version could not be
/// determined
pub(crate) fn supported_send_version() -> u32
{
    std::fs::read_to_string("/sys/fs/btrfs/features/send_stream_version")
        .map(|v| v.trim().parse::<u32>().unwrap_or(1))
        .unwrap_or(1)
}
