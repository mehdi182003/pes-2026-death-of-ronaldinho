//! IFP animation packages, version 1 (`ANPK`), as used by GTA III and Vice
//! City.
//!
//! Layout checked on Vice City's `anim/ped.ifp` and on the IFP files of
//! `gta3.img`, see `docs/formats/ifp.md`.

use std::fmt;

/// An animation package: `anim/ped.ifp` holds 234 animations.
#[derive(Debug, Clone, PartialEq)]
pub struct AnimationPackage {
    pub name: String,
    pub animations: Vec<Animation>,
}

impl AnimationPackage {
    /// Finds an animation by name, ignoring case.
    pub fn find(&self, name: &str) -> Option<&Animation> {
        self.animations
            .iter()
            .find(|animation| animation.name.eq_ignore_ascii_case(name))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Animation {
    pub name: String,
    /// One per animated bone (or prop).
    pub objects: Vec<AnimationObject>,
}

impl Animation {
    /// Time of the last key frame, in seconds.
    pub fn duration(&self) -> f32 {
        self.objects
            .iter()
            .filter_map(|object| object.keyframes.last())
            .map(|keyframe| keyframe.time)
            .fold(0.0, f32::max)
    }
}

/// Key frames of one bone.
#[derive(Debug, Clone, PartialEq)]
pub struct AnimationObject {
    /// Name of the bone, e.g. `Pelvis`.
    pub name: String,
    /// HAnim bone ID, as in the DFF frames. `None` when the file stores -1
    /// (toes, props) or uses the 48-byte variant: match by name then.
    pub bone_id: Option<i32>,
    /// Two indices stored instead of the bone ID by the 48-byte variant (32
    /// animations of ped.ifp, the GTA III layout).
    // HYPOTHÈSE: links to sibling objects ("next" and "previous" in the
    // GTAMods wiki); not used by Chaos FC.
    pub links: Option<[i32; 2]>,
    pub kind: KeyframeKind,
    pub keyframes: Vec<Keyframe>,
}

/// What each key frame of an object holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyframeKind {
    /// `KR00`: rotation.
    Rotation,
    /// `KRT0`: rotation and translation.
    RotationTranslation,
    /// `KRTS`: rotation, translation and scale.
    RotationTranslationScale,
}

impl KeyframeKind {
    fn from_tag(tag: [u8; 4]) -> Option<Self> {
        match &tag {
            b"KR00" => Some(Self::Rotation),
            b"KRT0" => Some(Self::RotationTranslation),
            b"KRTS" => Some(Self::RotationTranslationScale),
            _ => None,
        }
    }

    /// Bytes per key frame: 4 floats of rotation, 3 of translation, 3 of
    /// scale, then the time.
    fn stride(self) -> usize {
        match self {
            Self::Rotation => 20,
            Self::RotationTranslation => 32,
            Self::RotationTranslationScale => 44,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Keyframe {
    /// Quaternion (x, y, z, w) as stored: it is the **inverse** of the
    /// bone's rotation relative to its parent, see [`Keyframe::local_rotation`].
    pub rotation: [f32; 4],
    /// Position relative to the parent bone.
    pub translation: Option<[f32; 3]>,
    pub scale: Option<[f32; 3]>,
    /// Seconds from the start of the animation.
    pub time: f32,
}

impl Keyframe {
    /// Rotation of the bone relative to its parent, (x, y, z, w): the
    /// conjugate of the stored quaternion. On the first key frame of every
    /// animation of ped.ifp, the conjugate is the one close to the bind pose
    /// of player.dff (2100 bones against 162).
    pub fn local_rotation(&self) -> [f32; 4] {
        let [x, y, z, w] = self.rotation;
        [-x, -y, -z, w]
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IfpError {
    #[error("IFP tronqué à l'offset {offset:#x} : {what}")]
    Truncated { offset: usize, what: &'static str },

    #[error("IFP : section {expected} attendue à l'offset {offset:#x}, trouvé {found}")]
    UnexpectedSection {
        offset: usize,
        expected: &'static str,
        found: Tag,
    },

    #[error("IFP : {message} (offset {offset:#x})")]
    Invalid { offset: usize, message: String },
}

/// A four-character section tag, printable in error messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tag(pub [u8; 4]);

impl fmt::Display for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text: String = self.0.iter().map(|&b| char::from(b)).collect();
        write!(f, "« {} »", text.escape_debug())
    }
}

/// Parses an IFP file of version 1 (`ANPK`).
pub fn parse_ifp(bytes: &[u8]) -> Result<AnimationPackage, IfpError> {
    let reader = Reader { bytes, pos: 0 };
    let package = reader.section(Some(b"ANPK"), "ANPK")?;
    let mut reader = Reader {
        bytes: &bytes[..package.end],
        pos: package.data,
    };

    // The INFO section holds the count and the package name only: the
    // animations follow it, they are not inside.
    let info = reader.section(Some(b"INFO"), "INFO")?;
    let animation_count = reader.u32(info.data, "nombre d'animations")?;
    let name = reader.string(info.data + 4, info.end)?;
    reader.pos = info.next;

    let mut animations = Vec::new();
    for _ in 0..animation_count {
        let name_section = reader.section(Some(b"NAME"), "NAME")?;
        let name = reader.string(name_section.data, name_section.end)?;
        reader.pos = name_section.next;
        let data = reader.section(Some(b"DGAN"), "DGAN")?;
        let objects = parse_objects(bytes, &data)?;
        reader.pos = data.next;
        animations.push(Animation { name, objects });
    }
    Ok(AnimationPackage { name, animations })
}

/// Content of a DGAN section: an INFO (object count), then one CPAN per
/// object.
fn parse_objects(bytes: &[u8], dgan: &Section) -> Result<Vec<AnimationObject>, IfpError> {
    let mut reader = Reader {
        bytes: &bytes[..dgan.end],
        pos: dgan.data,
    };
    let info = reader.section(Some(b"INFO"), "INFO")?;
    let count = reader.u32(info.data, "nombre d'objets")?;
    reader.pos = info.next;

    let mut objects = Vec::new();
    for _ in 0..count {
        let cpan = reader.section(Some(b"CPAN"), "CPAN")?;
        let mut inner = Reader {
            bytes: &bytes[..cpan.end],
            pos: cpan.data,
        };
        let anim = inner.section(Some(b"ANIM"), "ANIM")?;
        let name = inner.string(anim.data, anim.data + 28)?;
        let frame_count = inner.u32(anim.data + 28, "nombre d'images")? as usize;
        // anim.data + 32: always 0 in Vice City.
        let last_frame = inner.u32(anim.data + 36, "dernière image")?;
        if frame_count == 0 || last_frame as usize != frame_count - 1 {
            return Err(IfpError::Invalid {
                offset: anim.data,
                message: format!("{frame_count} images, dernière image {last_frame}"),
            });
        }
        let (bone_id, links) = match anim.end - anim.data {
            // Vice City layout: the HAnim bone ID.
            44 => match inner.i32(anim.data + 40, "identifiant d'os")? {
                -1 => (None, None),
                id => (Some(id), None),
            },
            // GTA III layout: two links instead of the bone ID.
            48 => (
                None,
                Some([
                    inner.i32(anim.data + 40, "lien")?,
                    inner.i32(anim.data + 44, "lien")?,
                ]),
            ),
            size => {
                return Err(IfpError::Invalid {
                    offset: anim.data,
                    message: format!("section ANIM de {size} octets (44 ou 48 attendus)"),
                });
            }
        };
        inner.pos = anim.next;

        let keys = inner.section(None, "KR00, KRT0 ou KRTS")?;
        let kind = KeyframeKind::from_tag(keys.tag).ok_or(IfpError::UnexpectedSection {
            offset: keys.start,
            expected: "KR00, KRT0 ou KRTS",
            found: Tag(keys.tag),
        })?;
        let stride = kind.stride();
        if keys.end - keys.data != frame_count * stride {
            return Err(IfpError::Invalid {
                offset: keys.start,
                message: format!(
                    "{} octets d'images clés pour {frame_count} images de {stride} octets",
                    keys.end - keys.data
                ),
            });
        }
        let keyframes = (0..frame_count)
            .map(|index| read_keyframe(&bytes[keys.data + index * stride..][..stride], kind))
            .collect();

        objects.push(AnimationObject {
            name,
            bone_id,
            links,
            kind,
            keyframes,
        });
        reader.pos = cpan.next;
    }
    Ok(objects)
}

fn read_keyframe(bytes: &[u8], kind: KeyframeKind) -> Keyframe {
    let floats: Vec<f32> = bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|&b| f32::from_le_bytes(b))
        .collect();
    let vec3 = |at: usize| [floats[at], floats[at + 1], floats[at + 2]];
    Keyframe {
        rotation: [floats[0], floats[1], floats[2], floats[3]],
        translation: (kind != KeyframeKind::Rotation).then(|| vec3(4)),
        scale: (kind == KeyframeKind::RotationTranslationScale).then(|| vec3(7)),
        time: floats[floats.len() - 1],
    }
}

/// A section: 4-character tag, u32 size, then the data.
struct Section {
    tag: [u8; 4],
    /// Offset of the tag.
    start: usize,
    /// Offset of the data.
    data: usize,
    /// End of the data.
    end: usize,
    /// Start of the next section: the size does not count the padding of
    /// strings to 4 bytes, so the end is rounded up.
    next: usize,
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn u32(&self, at: usize, what: &'static str) -> Result<u32, IfpError> {
        self.bytes
            .get(at..at + 4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
            .ok_or(IfpError::Truncated { offset: at, what })
    }

    fn i32(&self, at: usize, what: &'static str) -> Result<i32, IfpError> {
        self.u32(at, what).map(|value| value as i32)
    }

    /// NUL-terminated string in `start..end`.
    fn string(&self, start: usize, end: usize) -> Result<String, IfpError> {
        let bytes = self.bytes.get(start..end).ok_or(IfpError::Truncated {
            offset: start,
            what: "chaîne",
        })?;
        Ok(crate::text::latin1_until_nul(bytes))
    }

    /// Reads the section header at the current position. `expected`: the
    /// tag it must have, if any.
    fn section(
        &self,
        expected: Option<&[u8; 4]>,
        expected_name: &'static str,
    ) -> Result<Section, IfpError> {
        let start = self.pos;
        let tag: [u8; 4] = self
            .bytes
            .get(start..start + 4)
            .ok_or(IfpError::Truncated {
                offset: start,
                what: expected_name,
            })?
            .try_into()
            .unwrap();
        if expected.is_some_and(|expected| *expected != tag) {
            return Err(IfpError::UnexpectedSection {
                offset: start,
                expected: expected_name,
                found: Tag(tag),
            });
        }
        let size = self.u32(start + 4, expected_name)? as usize;
        let data = start + 8;
        let end = data + size;
        if end > self.bytes.len() {
            return Err(IfpError::Truncated {
                offset: start,
                what: expected_name,
            });
        }
        Ok(Section {
            tag,
            start,
            data,
            end,
            next: (end + 3) & !3,
        })
    }
}

#[cfg(test)]
mod tests {
    //! Synthetic IFP files built in code: no game data.

    use super::*;

    fn section(tag: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut bytes = tag.to_vec();
        bytes.extend_from_slice(&(data.len() as u32).to_le_bytes());
        bytes.extend_from_slice(data);
        // Strings are padded to 4 bytes; the size does not count it.
        while !bytes.len().is_multiple_of(4) {
            bytes.push(0);
        }
        bytes
    }

    fn string(text: &str) -> Vec<u8> {
        let mut bytes = text.as_bytes().to_vec();
        bytes.push(0);
        bytes
    }

    fn floats(values: &[f32]) -> Vec<u8> {
        values.iter().flat_map(|v| v.to_le_bytes()).collect()
    }

    fn ints(values: &[i32]) -> Vec<u8> {
        values.iter().flat_map(|v| v.to_le_bytes()).collect()
    }

    /// ANIM section: name on 28 bytes, frame count, 0, last frame, then the
    /// bone ID (44 bytes) or two links (48 bytes).
    fn anim(name: &str, frames: i32, tail: &[i32]) -> Vec<u8> {
        let mut data = vec![0u8; 28];
        data[..name.len()].copy_from_slice(name.as_bytes());
        data.extend(ints(&[frames, 0, frames - 1]));
        data.extend(ints(tail));
        section(b"ANIM", &data)
    }

    fn package() -> Vec<u8> {
        // Root: two KRT0 key frames, bone ID 0.
        let root_keys = [
            floats(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0]),
            floats(&[0.5, 0.0, 0.0, 0.5, 0.0, 2.0, 1.0, 0.6]),
        ]
        .concat();
        let root = section(
            b"CPAN",
            &[anim("Root", 2, &[0]), section(b"KRT0", &root_keys)].concat(),
        );
        // A 48-byte ANIM with links, one KR00 key frame.
        let prop = section(
            b"CPAN",
            &[
                anim("Plane01", 1, &[-1, 2]),
                section(b"KR00", &floats(&[0.0, 0.0, 0.1, 0.9, 0.0])),
            ]
            .concat(),
        );
        let info = section(b"INFO", &[ints(&[2]), string("*")].concat());
        let dgan = section(b"DGAN", &[info, root, prop].concat());
        let body = [
            section(b"INFO", &[ints(&[1]), string("ped")].concat()),
            section(b"NAME", &string("run_player")),
            dgan,
        ]
        .concat();
        section(b"ANPK", &body)
    }

    #[test]
    fn parses_animations_objects_and_key_frames() {
        let package = parse_ifp(&package()).unwrap();
        assert_eq!(package.name, "ped");
        let animation = package.find("RUN_PLAYER").unwrap();
        assert_eq!(animation.name, "run_player");
        assert_eq!(animation.duration(), 0.6);

        let [root, prop] = animation.objects.as_slice() else {
            panic!("two objects expected");
        };
        assert_eq!(root.name, "Root");
        assert_eq!((root.bone_id, root.links), (Some(0), None));
        assert_eq!(root.kind, KeyframeKind::RotationTranslation);
        assert_eq!(root.keyframes[1].translation, Some([0.0, 2.0, 1.0]));
        assert_eq!(root.keyframes[1].scale, None);
        assert_eq!(root.keyframes[1].time, 0.6);
        assert_eq!(root.keyframes[1].local_rotation(), [-0.5, 0.0, 0.0, 0.5]);

        assert_eq!(prop.name, "Plane01");
        assert_eq!((prop.bone_id, prop.links), (None, Some([-1, 2])));
        assert_eq!(prop.kind, KeyframeKind::Rotation);
        assert_eq!(prop.keyframes[0].translation, None);
    }

    #[test]
    fn reads_scale_key_frames() {
        let keys = floats(&[0.0, 0.0, 0.0, 1.0, 1.0, 2.0, 3.0, 0.5, 0.5, 0.5, 0.25]);
        let cpan = section(
            b"CPAN",
            &[anim("Root", 1, &[0]), section(b"KRTS", &keys)].concat(),
        );
        let info = section(b"INFO", &[ints(&[1]), string("*")].concat());
        let body = [
            section(b"INFO", &[ints(&[1]), string("p")].concat()),
            section(b"NAME", &string("a")),
            section(b"DGAN", &[info, cpan].concat()),
        ]
        .concat();
        let package = parse_ifp(&section(b"ANPK", &body)).unwrap();
        let keyframe = package.animations[0].objects[0].keyframes[0];
        assert_eq!(keyframe.translation, Some([1.0, 2.0, 3.0]));
        assert_eq!(keyframe.scale, Some([0.5, 0.5, 0.5]));
        assert_eq!(keyframe.time, 0.25);
    }

    #[test]
    fn rejects_other_files_and_inconsistent_key_frames() {
        assert!(matches!(
            parse_ifp(b"ANP3\0\0\0\0"),
            Err(IfpError::UnexpectedSection { .. })
        ));

        // Key frames for 2 frames while ANIM announces 3.
        let keys = floats(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.1]);
        let cpan = section(
            b"CPAN",
            &[anim("Root", 3, &[0]), section(b"KR00", &keys)].concat(),
        );
        let info = section(b"INFO", &[ints(&[1]), string("*")].concat());
        let body = [
            section(b"INFO", &[ints(&[1]), string("p")].concat()),
            section(b"NAME", &string("a")),
            section(b"DGAN", &[info, cpan].concat()),
        ]
        .concat();
        let err = parse_ifp(&section(b"ANPK", &body)).unwrap_err();
        assert!(matches!(err, IfpError::Invalid { .. }), "{err}");

        let truncated = &package()[..100];
        assert!(parse_ifp(truncated).is_err());
    }
}
