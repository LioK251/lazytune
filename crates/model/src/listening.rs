use crate::PlaybackSnapshot;
use serde::{Deserialize, Serialize};
#[derive(Default)]
pub struct Listening {
    id: String,
    position: f64,
    last: f64,
    seconds: f64,
    playing: bool,
    submitted: bool,
    pub started: i64,
}
impl Listening {
    pub fn update(&mut self, p: &PlaybackSnapshot, monotonic: f64, unix: i64) -> bool {
        if p.track.id != self.id {
            *self = Self {
                id: p.track.id.clone(),
                position: p.position,
                last: monotonic,
                started: unix,
                ..Self::default()
            };
        }
        let elapsed = (monotonic - self.last).max(0.0);
        let moved = p.position - self.position;
        if self.playing && p.playing && !p.ad && moved >= 0.0 && (moved - elapsed).abs() < 1.5 {
            self.seconds += elapsed.min(moved);
        }
        self.position = p.position;
        self.last = monotonic;
        self.playing = p.playing && !p.ad;
        if !self.submitted
            && !self.id.is_empty()
            && p.duration > 30.0
            && self.seconds >= (p.duration / 2.0).min(240.0)
        {
            self.submitted = true;
            return true;
        }
        false
    }
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Segment {
    pub start: f64,
    pub end: f64,
    pub category: String,
}
pub fn clean_segments(mut segments: Vec<Segment>, duration: f64) -> Vec<Segment> {
    segments.retain(|s| {
        s.start.is_finite()
            && s.end.is_finite()
            && s.start >= 0.0
            && s.end > s.start
            && s.end <= duration
    });
    segments.sort_by(|a, b| a.start.total_cmp(&b.start));
    let mut result: Vec<Segment> = Vec::new();
    for s in segments {
        if let Some(last) = result.last_mut() {
            if s.start <= last.end {
                last.end = last.end.max(s.end);
                continue;
            }
        }
        result.push(s);
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn seeks_and_pause_do_not_scrobble() {
        let mut l = Listening::default();
        let mut p = PlaybackSnapshot::default();
        p.track.id = "dQw4w9WgXcQ".into();
        p.duration = 60.0;
        p.playing = true;
        for t in 0..10 {
            p.position = t as f64;
            assert!(!l.update(&p, t as f64, 1000 + t));
        }
        p.position = 45.0;
        assert!(!l.update(&p, 10.0, 1010));
        p.playing = false;
        assert!(!l.update(&p, 40.0, 1040));
        p.playing = true;
        let mut count = 0;
        for t in 41..75 {
            p.position += 1.0;
            if l.update(&p, t as f64, 1000 + t) {
                count += 1;
            }
        }
        assert_eq!(count, 1);
    }
    #[test]
    fn invalid_and_overlapping_segments() {
        let s = clean_segments(
            vec![
                Segment {
                    start: 1.0,
                    end: 5.0,
                    category: "sponsor".into(),
                },
                Segment {
                    start: 3.0,
                    end: 8.0,
                    category: "sponsor".into(),
                },
                Segment {
                    start: f64::NAN,
                    end: 1.0,
                    category: "sponsor".into(),
                },
            ],
            20.0,
        );
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].end, 8.0);
    }
    #[test]
    fn network_delays_count_playback_but_sleep_does_not() {
        let mut listening = Listening::default();
        let mut p = PlaybackSnapshot::default();
        p.track.id = "dQw4w9WgXcQ".into();
        p.duration = 60.0;
        p.playing = true;
        assert!(!listening.update(&p, 0.0, 100));
        p.position = 20.0;
        assert!(!listening.update(&p, 20.0, 120));
        assert!(!listening.update(&p, 200.0, 300));
        p.position = 30.0;
        assert!(listening.update(&p, 210.0, 310));
    }
}
