//! File type categories. The numbers are stored in `Node::kind` and in the scan cache,
//! so only add new values at the end.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
#[repr(u8)]
pub enum Kind {
    Other = 0,
    Video = 1,
    Image = 2,
    Doc = 3,
    App = 4,
    Game = 5,
    Archive = 6,
    System = 7,
    Dev = 8,
}

pub const KIND_COUNT: usize = 9;

impl Kind {
    pub fn from_u8(v: u8) -> Kind {
        match v {
            1 => Kind::Video,
            2 => Kind::Image,
            3 => Kind::Doc,
            4 => Kind::App,
            5 => Kind::Game,
            6 => Kind::Archive,
            7 => Kind::System,
            8 => Kind::Dev,
            _ => Kind::Other,
        }
    }
}

/// Category from the file extension alone.
pub fn kind_of_name(name: &str) -> Kind {
    let Some(dot) = name.rfind('.') else {
        return Kind::Other;
    };
    let ext = name[dot + 1..].to_ascii_lowercase();
    match ext.as_str() {
        "mp4" | "mkv" | "mov" | "avi" | "wmv" | "webm" | "m4v" | "flv" | "mpg" | "mpeg"
        | "m2ts" | "mts" | "3gp" | "mp3" | "flac" | "wav" | "aac" | "ogg" | "m4a" | "wma"
        | "opus" | "aiff" => Kind::Video,
        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "tif" | "tiff" | "heic" | "heif" | "webp"
        | "avif" | "raw" | "cr2" | "cr3" | "nef" | "arw" | "dng" | "orf" | "rw2" | "psd"
        | "psb" | "svg" | "ico" | "xcf" | "kra" | "lrcat" | "blend" => Kind::Image,
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "odt" | "ods" | "odp"
        | "rtf" | "txt" | "md" | "csv" | "epub" | "mobi" | "one" | "pst" | "ost" | "eml"
        | "msg" | "pages" | "numbers" | "key" | "tex" => Kind::Doc,
        "exe" | "msi" | "msix" | "msixbundle" | "appx" | "appxbundle" | "lnk" | "url" => Kind::App,
        "pak" | "vpk" | "bsa" | "ba2" | "forge" | "wad" | "upk" | "uasset" | "umap" | "utoc"
        | "ucas" | "xnb" | "gcf" | "rpf" | "big" | "esm" | "esp" | "bk2" | "wem" | "bnk" => {
            Kind::Game
        }
        "zip" | "rar" | "7z" | "tar" | "gz" | "tgz" | "bz2" | "xz" | "zst" | "lz4" | "cab"
        | "iso" | "img" | "vhd" | "vhdx" | "vmdk" | "vdi" | "qcow2" | "wim" | "esd" | "dmg"
        | "swm" => Kind::Archive,
        "dll" | "sys" | "drv" | "mui" | "cat" | "inf" | "etl" | "evtx" | "regtrans-ms" | "blf"
        | "msp" | "manifest" | "mum" | "ocx" | "cpl" | "efi" | "ttf" | "otf" | "fon" | "edb"
        | "jrs" => Kind::System,
        "rs" | "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" | "py" | "pyc" | "pyd" | "c" | "cc"
        | "cpp" | "h" | "hpp" | "java" | "class" | "jar" | "kt" | "go" | "cs" | "csproj"
        | "sln" | "fs" | "swift" | "rb" | "php" | "lua" | "dart" | "scala" | "o" | "obj"
        | "pdb" | "lib" | "a" | "rlib" | "rmeta" | "ilk" | "idb" | "pch" | "ipch" | "whl"
        | "map" | "lock" | "toml" | "yaml" | "yml" | "json" | "node" | "wasm" | "d" | "tlog" => {
            Kind::Dev
        }
        _ => Kind::Other,
    }
}

/// A folder name that puts every file below it in one category.
/// Media, documents, images and archives keep their own category.
pub fn zone_of_dir(name: &str, depth: usize) -> Option<Kind> {
    let n = name.to_ascii_lowercase();
    match n.as_str() {
        "windows" if depth == 1 => Some(Kind::System),
        "program files" | "program files (x86)" if depth == 1 => Some(Kind::App),
        "programs" => Some(Kind::App),
        "steamapps"
        | "xboxgames"
        | "epic games"
        | "riot games"
        | "gog games"
        | "gog galaxy"
        | "ubisoft game launcher"
        | "ea games"
        | "battle.net" => Some(Kind::Game),
        "node_modules" | "target" | ".venv" | "venv" | "__pycache__" | ".gradle" | ".cargo"
        | ".rustup" | ".npm" | ".nuget" | ".m2" | "site-packages" | ".git" => Some(Kind::Dev),
        _ => None,
    }
}

/// Applies a zone to a file category.
pub fn apply_zone(kind: Kind, zone: Option<Kind>) -> Kind {
    match (kind, zone) {
        (Kind::Other | Kind::System | Kind::App | Kind::Dev, Some(z)) => z,
        (k, _) => k,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extensions_map_to_categories() {
        assert_eq!(kind_of_name("Trip.MP4"), Kind::Video);
        assert_eq!(kind_of_name("photo.heic"), Kind::Image);
        assert_eq!(kind_of_name("report.pdf"), Kind::Doc);
        assert_eq!(kind_of_name("setup.exe"), Kind::App);
        assert_eq!(kind_of_name("ubuntu.iso"), Kind::Archive);
        assert_eq!(kind_of_name("pagefile.sys"), Kind::System);
        assert_eq!(kind_of_name("main.rs"), Kind::Dev);
        assert_eq!(kind_of_name("README"), Kind::Other);
        assert_eq!(kind_of_name("archive.tar.gz"), Kind::Archive);
    }

    #[test]
    fn zones_override_weak_categories_only() {
        let game = zone_of_dir("steamapps", 4);
        assert_eq!(apply_zone(Kind::System, game), Kind::Game);
        assert_eq!(apply_zone(Kind::Video, game), Kind::Video);
        assert_eq!(zone_of_dir("Windows", 1), Some(Kind::System));
        assert_eq!(zone_of_dir("Windows", 3), None);
    }

    #[test]
    fn kind_round_trips_through_u8() {
        for v in 0..KIND_COUNT as u8 {
            assert_eq!(Kind::from_u8(v) as u8, v);
        }
    }
}
