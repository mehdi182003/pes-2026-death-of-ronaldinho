//! `data/weapon.dat` of Vice City: one line of 26 fields per weapon.
//!
//! Layout from the GTAMods wiki ("weapon.dat", Vice City table), checked on
//! the player's file, see `docs/formats/weapon-dat.md`.

/// Number of fields of a weapon line in Vice City.
pub const FIELD_COUNT: usize = 26;

/// One weapon, as described by `weapon.dat`.
#[derive(Debug, Clone, PartialEq)]
pub struct WeaponInfo {
    /// Hardcoded weapon name, e.g. `Colt45`.
    pub name: String,
    /// `MELEE`, `INSTANT_HIT`, `PROJECTILE`, `AREA_EFFECT` or `CAMERA`.
    pub fire_type: String,
    /// Range of the shot, in metres.
    pub range: f32,
    /// Meaning not documented; 250 for most guns.
    pub firing_rate: i32,
    /// Reload time, in milliseconds.
    pub reload: i32,
    /// Rounds per clip.
    pub ammo: i32,
    pub damage: i32,
    pub speed: f32,
    pub radius: f32,
    pub life_span: f32,
    pub spread: f32,
    /// Where the gunflash appears, relative to the weapon.
    pub fire_offset: [f32; 3],
    /// Animation package of `gta3.img` (e.g. `colt45` → `colt45.ifp`).
    pub anim_group: String,
    /// Shooting animation: loop start, loop end and firing point, in frames
    /// at 30 per second (Colt45: 11, 18, 14 fall inside colt45_fire, whose
    /// keys are 1/30 s apart; the shooting rhythm was validated by eye).
    pub anim_loop: [f32; 3],
    /// Same for the crouching animation.
    pub anim2_loop: [f32; 3],
    /// Point where the attack can be interrupted to run away.
    pub breakout: f32,
    /// IDE model of the weapon (e.g. 274 for the Colt 45).
    pub model_id: i32,
    /// IDE model of an attachment, -1 if none.
    pub model2_id: i32,
    /// Flags, written in hexadecimal in the file.
    pub flags: u32,
    /// Weapon slot: 3 pistol, 5 submachine gun, 6 assault rifle...
    pub slot: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("weapon.dat, ligne {line} : {message}")]
pub struct WeaponDatError {
    pub line: usize,
    pub message: String,
}

/// Parses `weapon.dat`. Comments (`#`) and empty lines are skipped; reading
/// stops at `ENDWEAPONDATA`.
pub fn parse_weapon_dat(text: &str) -> Result<Vec<WeaponInfo>, WeaponDatError> {
    let mut weapons = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line_number = index + 1;
        let content = line.split('#').next().unwrap_or("").trim();
        if content.is_empty() {
            continue;
        }
        if content.starts_with("ENDWEAPONDATA") {
            return Ok(weapons);
        }
        weapons.push(parse_line(content, line_number)?);
    }
    Err(WeaponDatError {
        line: text.lines().count(),
        message: "ENDWEAPONDATA manquant".into(),
    })
}

fn parse_line(content: &str, line: usize) -> Result<WeaponInfo, WeaponDatError> {
    let fields: Vec<&str> = content.split_whitespace().collect();
    if fields.len() != FIELD_COUNT {
        return Err(WeaponDatError {
            line,
            message: format!("{} champs au lieu de {FIELD_COUNT}", fields.len()),
        });
    }
    let error = |index: usize, kind: &str| WeaponDatError {
        line,
        message: format!("champ {} « {} » : {kind} attendu", index + 1, fields[index]),
    };
    let float = |index: usize| {
        fields[index]
            .parse::<f32>()
            .map_err(|_| error(index, "nombre"))
    };
    let int = |index: usize| {
        fields[index]
            .parse::<i32>()
            .map_err(|_| error(index, "entier"))
    };

    Ok(WeaponInfo {
        name: fields[0].to_owned(),
        fire_type: fields[1].to_owned(),
        range: float(2)?,
        firing_rate: int(3)?,
        reload: int(4)?,
        ammo: int(5)?,
        damage: int(6)?,
        speed: float(7)?,
        radius: float(8)?,
        life_span: float(9)?,
        spread: float(10)?,
        fire_offset: [float(11)?, float(12)?, float(13)?],
        anim_group: fields[14].to_owned(),
        anim_loop: [float(15)?, float(16)?, float(17)?],
        anim2_loop: [float(18)?, float(19)?, float(20)?],
        breakout: float(21)?,
        model_id: int(22)?,
        model2_id: int(23)?,
        flags: u32::from_str_radix(fields[24], 16).map_err(|_| error(24, "nombre hexadécimal"))?,
        slot: int(25)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "#\n\
        # comment\n\
        Unarmed \tMELEE 2.4 250 100 1000 8 -1.0 0.6 -1.0 -1.0\t0.1 0.65\t0.30\tunarmed\t0 99 6\t0 99 12\t99\t\t-1 -1\t102000\t0\n\
        \n\
        Colt45 \tINSTANT_HIT 30.0  250  450  17   25  -1.0 -1.0  -1.0   -1.0\t0.30 0.0\t0.09\tcolt45 \t11 18 14 \t11 18 12\t99\t\t274 -1\t  680C0\t\t3 # pistol\n\
        ENDWEAPONDATA\n\
        Ignored line after the end\n";

    #[test]
    fn parses_weapon_lines() {
        let weapons = parse_weapon_dat(SAMPLE).unwrap();
        assert_eq!(weapons.len(), 2);
        let colt = &weapons[1];
        assert_eq!(colt.name, "Colt45");
        assert_eq!(colt.fire_type, "INSTANT_HIT");
        assert_eq!((colt.range, colt.reload, colt.ammo), (30.0, 450, 17));
        assert_eq!(colt.fire_offset, [0.30, 0.0, 0.09]);
        assert_eq!(colt.anim_group, "colt45");
        assert_eq!(colt.anim_loop, [11.0, 18.0, 14.0]);
        assert_eq!(colt.anim2_loop, [11.0, 18.0, 12.0]);
        assert_eq!((colt.model_id, colt.model2_id), (274, -1));
        assert_eq!(colt.flags, 0x680C0);
        assert_eq!(colt.slot, 3);
        assert_eq!(weapons[0].flags, 0x102000);
    }

    #[test]
    fn reports_bad_lines() {
        let err = parse_weapon_dat("Colt45 INSTANT_HIT 30.0\nENDWEAPONDATA\n").unwrap_err();
        assert_eq!(err.line, 1);
        assert!(err.message.contains("3 champs"), "{err}");

        let bad_number = SAMPLE.replace("30.0  250", "trente  250");
        let err = parse_weapon_dat(&bad_number).unwrap_err();
        assert_eq!(err.line, 5);

        let err = parse_weapon_dat("# only comments\n").unwrap_err();
        assert!(err.message.contains("ENDWEAPONDATA"), "{err}");
    }
}
