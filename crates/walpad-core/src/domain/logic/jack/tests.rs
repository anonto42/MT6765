use super::*;

#[test]
fn detected_wins_over_forced() {
    assert_eq!(
        effective(Some(JackKind::Headphones), Some(JackKind::HeadsetWithMic)),
        Some(JackKind::Headphones)
    );
}

#[test]
fn forced_is_used_without_detection() {
    assert_eq!(
        effective(None, Some(JackKind::HeadsetWithMic)),
        Some(JackKind::HeadsetWithMic)
    );
}

#[test]
fn neither_means_speaker_and_builtin_mic() {
    assert_eq!(effective(None, None), None);
    assert_eq!(route_for(None), Route::Speaker);
    assert_eq!(mic_source_for(None), MicSource::BuiltIn);
}

#[test]
fn any_kind_routes_output_to_headphone() {
    assert_eq!(route_for(Some(JackKind::Headphones)), Route::Headphone);
    assert_eq!(route_for(Some(JackKind::HeadsetWithMic)), Route::Headphone);
}

#[test]
fn only_headset_with_mic_moves_the_mic() {
    assert_eq!(
        mic_source_for(Some(JackKind::Headphones)),
        MicSource::BuiltIn
    );
    assert_eq!(
        mic_source_for(Some(JackKind::HeadsetWithMic)),
        MicSource::HeadsetMic
    );
}

#[test]
fn full_transition_sequence() {
    // absent -> headphones only (output moves, mic stays built-in) -> headset with mic
    // (mic also moves) -> back to absent.
    let steps = [
        (None, Route::Speaker, MicSource::BuiltIn),
        (
            Some(JackKind::Headphones),
            Route::Headphone,
            MicSource::BuiltIn,
        ),
        (
            Some(JackKind::HeadsetWithMic),
            Route::Headphone,
            MicSource::HeadsetMic,
        ),
        (None, Route::Speaker, MicSource::BuiltIn),
    ];
    for (kind, route, mic) in steps {
        assert_eq!(route_for(kind), route, "{kind:?}");
        assert_eq!(mic_source_for(kind), mic, "{kind:?}");
    }
}
