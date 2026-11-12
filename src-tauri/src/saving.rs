//! Where the files Dawwin makes go: next to the recording, as it comes, or into one folder of the
//! person's own (Settings, "Where files go"). It decides where the steps save the sound and the
//! video with subtitles, and where the save dialogs of the exports open. A folder run keeps
//! writing next to each recording: that is what it is for.

use std::path::{Path, PathBuf};

use crate::setup::Settings;

/// The folder chosen in Settings, when one is and it is still there.
pub fn own_folder(settings: &Settings) -> Option<PathBuf> {
    settings.save_dir.as_ref().map(PathBuf::from).filter(|d| d.is_dir())
}

/// Where a file that would go next to the recording goes: the same name, in the folder of one's
/// own when there is one, except for a folder run.
pub fn place(settings: &Settings, next_to_recording: PathBuf, folder_run: bool) -> PathBuf {
    match (own_folder(settings), next_to_recording.file_name()) {
        (Some(dir), Some(name)) if !folder_run => dir.join(name),
        _ => next_to_recording,
    }
}

/// Where a save dialog opens: in the folder of one's own, else in the recording's.
pub fn dialog_folder(settings: &Settings, recording: &Path) -> Option<PathBuf> {
    own_folder(settings).or_else(|| recording.parent().filter(|d| d.is_dir()).map(Path::to_path_buf))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_go_next_to_the_recording_or_to_the_folder_of_ones_own() {
        let mine = std::env::temp_dir().join(format!("dawwin-saving-{}", std::process::id()));
        std::fs::create_dir_all(&mine).unwrap();
        let recording = Path::new(r"D:\Lessons\talk.mp4");
        let sound = recording.with_file_name("talk.m4a");
        let mut s = Settings::default();
        assert_eq!(place(&s, sound.clone(), false), sound, "as it comes: next to the recording");
        s.save_dir = Some(mine.display().to_string());
        assert_eq!(place(&s, sound.clone(), false), mine.join("talk.m4a"));
        assert_eq!(place(&s, sound.clone(), true), sound, "a folder run writes next to each recording");
        assert_eq!(dialog_folder(&s, recording), Some(mine.clone()));
        std::fs::remove_dir_all(&mine).unwrap();
        assert_eq!(place(&s, sound.clone(), false), sound, "a folder of one's own that is gone: next to the recording");
    }
}
