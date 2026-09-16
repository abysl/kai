#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Bundle,
    Gateway,
}

#[derive(Debug)]
pub struct Selection {
    source: Option<Source>,
    pending: bool,
}

impl Default for Selection {
    fn default() -> Self {
        Self {
            source: None,
            pending: true,
        }
    }
}

impl Selection {
    pub fn adopt(&mut self, source: Source) -> bool {
        if self.source == Some(Source::Bundle) && source == Source::Gateway {
            return false;
        }
        self.source = Some(source);
        true
    }

    pub fn bundle_finished(&mut self) {
        self.pending = false;
    }

    pub fn bundle_pending(&self) -> bool {
        self.pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bundle_wins_in_either_download_order() {
        for sources in [
            [Source::Bundle, Source::Gateway],
            [Source::Gateway, Source::Bundle],
        ] {
            let mut selection = Selection::default();
            for source in sources {
                let _ = selection.adopt(source);
            }
            assert_eq!(selection.source, Some(Source::Bundle));
            assert!(!selection.adopt(Source::Gateway));
        }
    }

    #[test]
    fn a_gateway_cannot_make_hosting_ready_before_the_bundle_finishes() {
        let mut selection = Selection::default();
        assert!(selection.adopt(Source::Gateway));
        assert!(selection.bundle_pending());
        selection.bundle_finished();
        assert!(!selection.bundle_pending());
    }

    #[test]
    fn a_failed_bundle_still_allows_the_gateway_fallback() {
        let mut selection = Selection::default();
        selection.bundle_finished();
        assert!(selection.adopt(Source::Gateway));
        assert_eq!(selection.source, Some(Source::Gateway));
    }
}
