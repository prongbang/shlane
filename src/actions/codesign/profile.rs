//! Reading a `.mobileprovision`.
//!
//! The file is a CMS envelope wrapped around a plist. Parsing CMS to get at
//! four strings would be a lot of machinery for no gain, so the plist is found
//! inside the envelope and read directly — which is what every other tool that
//! does this ends up doing.

/// Pull the plist out of the signed container.
pub fn extract_plist(contents: &[u8]) -> Option<&str> {
    let start = find(contents, b"<?xml")?;
    let end = find(&contents[start..], b"</plist>")? + start + "</plist>".len();
    std::str::from_utf8(&contents[start..end]).ok()
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Read `<key>Name</key><string>value</string>`.
pub fn string_field(plist: &str, key: &str) -> Option<String> {
    let after_key = after_key(plist, key)?;
    let start = after_key.find("<string>")? + "<string>".len();
    let end = after_key[start..].find("</string>")? + start;
    Some(unescape(&after_key[start..end]))
}

/// Read the first entry of `<key>Name</key><array><string>value</string>...`.
pub fn first_array_field(plist: &str, key: &str) -> Option<String> {
    let after_key = after_key(plist, key)?;
    let array = after_key.find("<array>")?;
    let start = after_key[array..].find("<string>")? + array + "<string>".len();
    let end = after_key[start..].find("</string>")? + start;
    Some(unescape(&after_key[start..end]))
}

fn after_key<'a>(plist: &'a str, key: &str) -> Option<&'a str> {
    let marker = format!("<key>{key}</key>");
    let at = plist.find(&marker)? + marker.len();
    Some(&plist[at..])
}

fn unescape(value: &str) -> String {
    value
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// What a lane needs to know about a profile it just installed.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Profile {
    pub uuid: String,
    pub name: String,
    pub team_id: String,
    pub app_id: String,
}

pub fn read(contents: &[u8]) -> Result<Profile, String> {
    let plist = extract_plist(contents)
        .ok_or_else(|| "no plist found inside the provisioning profile".to_string())?;

    let uuid = string_field(plist, "UUID")
        .ok_or_else(|| "the provisioning profile has no UUID".to_string())?;

    Ok(Profile {
        uuid,
        name: string_field(plist, "Name").unwrap_or_default(),
        team_id: first_array_field(plist, "TeamIdentifier").unwrap_or_default(),
        app_id: string_field(plist, "AppIDName").unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shape of a real profile: a signed container with the plist inside.
    fn fixture() -> Vec<u8> {
        let mut bytes = vec![0x30, 0x82, 0x0a, 0x00]; // CMS header bytes
        bytes.extend_from_slice(
            br#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
	<key>AppIDName</key>
	<string>Acme &amp; Co</string>
	<key>TeamIdentifier</key>
	<array>
		<string>ABCDE12345</string>
	</array>
	<key>Name</key>
	<string>AppStore com.example.app</string>
	<key>UUID</key>
	<string>1a2b3c4d-0000-1111-2222-333344445555</string>
</dict>
</plist>"#,
        );
        bytes.extend_from_slice(&[0x00, 0x01, 0x02]); // trailing signature bytes
        bytes
    }

    #[test]
    fn finds_the_plist_inside_the_container() {
        let bytes = fixture();
        let plist = extract_plist(&bytes).expect("a plist");
        assert!(plist.starts_with("<?xml"), "{plist}");
        assert!(plist.ends_with("</plist>"), "{plist}");
    }

    #[test]
    fn reads_the_fields_a_lane_needs() {
        let profile = read(&fixture()).expect("a profile");
        assert_eq!(profile.uuid, "1a2b3c4d-0000-1111-2222-333344445555");
        assert_eq!(profile.name, "AppStore com.example.app");
        assert_eq!(profile.team_id, "ABCDE12345");
        assert_eq!(profile.app_id, "Acme & Co");
    }

    #[test]
    fn a_profile_without_a_uuid_is_reported() {
        let error = read(b"<?xml ?><plist></plist>").expect_err("should fail");
        assert!(error.contains("UUID"), "{error}");
    }

    #[test]
    fn something_that_is_not_a_profile_is_reported() {
        assert!(read(b"not a profile at all").is_err());
    }
}
