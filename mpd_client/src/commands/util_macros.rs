macro_rules! field {
    ($frame:ident, $field:literal $type:ident) => {
        field!($frame, $field $type optional)
            .ok_or($crate::commands::TypedResponseError {
                field: $field,
                kind: $crate::commands::ErrorKind::Missing,
            })?
    };
    ($frame:ident, $field:literal $type:ident optional) => {
        match $frame.get($field) {
            None => None,
            Some(val) => Some(parse!($type, val, $field))
        }
    };
    ($frame:ident, $field:literal $type:ident default $default:expr) => {
        field!($frame, $field $type optional).unwrap_or($default)
    };
}

macro_rules! parse {
    (integer, $value:ident, $field:literal) => {
        $value
            .parse()
            .map_err(|e| $crate::commands::TypedResponseError {
                field: $field,
                kind: $crate::commands::ErrorKind::MalformedInteger(e),
            })?
    };
    (PlayState, $value:ident, $field:literal) => {
        match $value.as_str() {
            "play" => PlayState::Playing,
            "pause" => PlayState::Paused,
            "stop" => PlayState::Stopped,
            _ => {
                return Err($crate::commands::TypedResponseError {
                    field: $field,
                    kind: $crate::commands::ErrorKind::InvalidValue($value),
                })
            }
        }
    };
    (boolean, $value:ident, $field:literal) => {
        match $value.as_str() {
            "1" => true,
            "0" => false,
            _ => {
                return Err($crate::commands::TypedResponseError {
                    field: $field,
                    kind: $crate::commands::ErrorKind::InvalidValue($value),
                })
            }
        }
    };
    (duration, $value:ident, $field:literal) => {
        $crate::commands::responses::parse_duration($field, &$value)?
    };
}

macro_rules! song_identifier {
    ($frame:ident, $position:literal, $id:literal) => {
        {
            let pos = field!($frame, $position integer optional);
            let id = field!($frame, $id integer optional);

            match (pos, id) {
                (Some(pos), Some(id)) => Some((SongPosition(pos), SongId(id))),
                _ => None,
            }
        }
    };
}
