//! Android paths that Lustro never deletes. Lustro sees only the shared storage: the
//! internal storage of each user (`/storage/emulated/<user>`) and SD cards and USB drives
//! (`/storage/<volume>`). Shared storage ignores case, so the tables are in lower case.

use super::unix::Rules;

pub const RULES: Rules = Rules {
    // The system: an app cannot write there, but a rooted phone can.
    system_trees: &[
        "acct",
        "apex",
        "cache",
        "config",
        "data",
        "dev",
        "metadata",
        "mnt",
        "odm",
        "proc",
        "product",
        "sdcard",
        "storage/self",
        "sys",
        "system",
        "system_ext",
        "vendor",
    ],
    system_items: &["storage"],
    // Each volume is mounted at /storage/<name>; the internal storage at /storage/emulated.
    mount_parents: &["storage"],
    // The private data of apps on an SD card. Android does not let other apps read it.
    volume_trees: &["android/data", "android/obb"],
    // /storage/emulated/<user> is the home folder of each user.
    homes: "storage/emulated",
    root_home: "/data",
    home_trees: &["android/data", "android/obb"],
    home_items: &[
        "alarms",
        "android",
        "android/media",
        "audiobooks",
        "dcim",
        "documents",
        "download",
        "movies",
        "music",
        "notifications",
        "pictures",
        "podcasts",
        "recordings",
        "ringtones",
    ],
    cleaner_exceptions: &[],
    app_trees: &[],
    ignore_case: true,
};

#[cfg(test)]
mod tests {
    use super::super::unix;
    use super::RULES;

    fn protected(path: &str) -> bool {
        unix::protected(&RULES, path, None)
    }

    #[test]
    fn system_and_volume_roots_are_protected() {
        for p in [
            "/",
            "/system/app/Gallery.apk",
            "/data/data/com.example",
            "/sdcard/Download/a.apk",
            "/storage",
            "/storage/emulated",
            "/storage/self/primary",
            "/storage/emulated/0",
            "/storage/emulated/10",
            "/storage/ABCD-1234",
        ] {
            assert!(protected(p), "{p} is protected");
        }
    }

    #[test]
    fn app_data_is_protected_on_every_volume() {
        for p in [
            "/storage/emulated/0/Android",
            "/storage/emulated/0/Android/data",
            "/storage/emulated/0/Android/data/com.example/files/a.bin",
            "/storage/emulated/0/android/OBB/game.obb",
            "/storage/ABCD-1234/Android/data/com.example/cache",
            "/storage/ABCD-1234/Android/obb",
        ] {
            assert!(protected(p), "{p} is protected");
        }
    }

    #[test]
    fn standard_folders_stay_but_their_contents_do_not() {
        for p in [
            "/storage/emulated/0/DCIM",
            "/storage/emulated/0/Download",
            "/storage/emulated/0/Android/media",
        ] {
            assert!(protected(p), "{p} is protected");
        }
        for p in [
            "/storage/emulated/0/DCIM/Camera/IMG_1.jpg",
            "/storage/emulated/0/Download/old.apk",
            "/storage/emulated/0/Android/media/com.whatsapp/WhatsApp/Media",
            "/storage/emulated/0/MyFolder",
            "/storage/ABCD-1234/DCIM/Camera/IMG_2.jpg",
            "/storage/ABCD-1234/Android/media/x",
        ] {
            assert!(!protected(p), "{p} is not protected");
        }
    }
}
