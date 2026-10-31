use crate::Error;

pub(super) fn ico(bytes: &[u8]) -> Result<Vec<u8>, Error> {
    if bytes.starts_with(b"\0\0\x01\0") {
        resources(bytes)?;
        return Ok(bytes.to_vec());
    }
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") || bytes.len() < 33 || &bytes[12..16] != b"IHDR" {
        return Err(Error::InvalidInput("icon: expected PNG or ICO".into()));
    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
    let height = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
    if !(1..=256).contains(&width) || !(1..=256).contains(&height) {
        return Err(Error::InvalidInput(
            "icon: PNG dimensions must be 1 to 256 pixels".into(),
        ));
    }
    let length = u32::try_from(bytes.len())
        .map_err(|_| Error::InvalidInput("icon: PNG is too large".into()))?;
    let mut result = vec![
        0,
        0,
        1,
        0,
        1,
        0,
        width as u8,
        height as u8,
        0,
        0,
        1,
        0,
        32,
        0,
    ];
    result.extend(length.to_le_bytes());
    result.extend(22u32.to_le_bytes());
    result.extend(bytes);
    Ok(result)
}

pub(super) fn resources(ico: &[u8]) -> Result<(Vec<u8>, Vec<&[u8]>), Error> {
    let invalid = || Error::InvalidInput("icon: invalid ICO directory or image range".into());
    if ico.len() < 6 || ico[..4] != [0, 0, 1, 0] {
        return Err(invalid());
    }
    let count = u16::from_le_bytes([ico[4], ico[5]]) as usize;
    let end = 6 + count * 16;
    if count == 0 || ico.len() < end {
        return Err(invalid());
    }
    let mut group = ico[..6].to_vec();
    let mut images = Vec::with_capacity(count);
    for (index, entry) in ico[6..end].as_chunks::<16>().0.iter().enumerate() {
        let length = u32::from_le_bytes(entry[8..12].try_into().unwrap()) as usize;
        let offset = u32::from_le_bytes(entry[12..16].try_into().unwrap()) as usize;
        let range_end = offset.checked_add(length).ok_or_else(invalid)?;
        if length == 0 || offset < end || range_end > ico.len() {
            return Err(invalid());
        }
        images.push(&ico[offset..range_end]);
        group.extend(&entry[..12]);
        group.extend(((index + 1) as u16).to_le_bytes());
    }
    Ok((group, images))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_round_trips_into_icon_resources() {
        let png = include_bytes!("../../../assets/icon.png");
        let icon = ico(png).unwrap();
        assert_eq!(ico(&icon).unwrap(), icon);
        let (group, images) = resources(&icon).unwrap();
        assert_eq!(images, [png.as_slice()]);
        assert_eq!(group.len(), 20);
        assert_eq!(&group[18..20], &[1, 0]);
        assert_eq!(&icon[18..22], &22u32.to_le_bytes());
    }

    #[test]
    fn rejects_truncation_bad_ranges_and_oversize_png() {
        let icon = ico(include_bytes!("../../../assets/icon.png")).unwrap();
        for length in 0..icon.len() {
            assert!(resources(&icon[..length]).is_err());
        }
        let mut invalid = icon.clone();
        invalid[18..22].copy_from_slice(&0u32.to_le_bytes());
        assert!(resources(&invalid).is_err());
        let mut png = include_bytes!("../../../assets/icon.png").to_vec();
        png[16..20].copy_from_slice(&257u32.to_be_bytes());
        assert!(ico(&png).is_err());
        assert!(ico(b"not an icon").is_err());
    }

    #[test]
    fn windows_targets_reject_unsupported_packages_and_install() {
        use crate::{Platform, Target};
        for target in [Target::App, Target::Zip] {
            assert!(Platform::Windows.validate(target, false).is_ok());
            assert!(Platform::Windows.validate(target, true).is_err());
        }
        for target in [Target::Dmg, Target::Deb, Target::Tarball] {
            assert!(Platform::Windows.validate(target, false).is_err());
        }
        for platform in [Platform::Macos, Platform::Linux] {
            assert!(platform.validate(Target::Zip, false).is_err());
        }
    }
}
