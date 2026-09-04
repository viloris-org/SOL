use reedline::{Completer, Suggestion};

/// Adapts another completer for Reedline's horizontal columnar menu.
///
/// Reedline 0.37 already lays suggestions out in row-major order. However, it
/// switches [`reedline::ColumnarMenu`] to a single column as soon as any
/// suggestion has a description. Lyra's completers attach descriptions to
/// almost every suggestion, so configuring more columns on the menu alone has
/// no visible effect.
///
/// This wrapper preserves the inner completer's ranking and replacement data,
/// but removes descriptions when a multi-column layout is requested. This
/// keeps the suggestions eligible for Reedline's horizontal grid.
pub struct HorizontalCompleter<C> {
    inner: C,
    columns: u16,
}

impl<C> HorizontalCompleter<C> {
    /// Wrap a completer for a menu with `columns` columns.
    ///
    /// # Panics
    ///
    /// Panics when `columns` is zero, which is not a valid menu layout.
    pub fn new(inner: C, columns: u16) -> Self {
        assert!(columns > 0, "a completion menu needs at least one column");
        Self { inner, columns }
    }

    /// Return a shared reference to the wrapped completer.
    pub fn inner(&self) -> &C {
        &self.inner
    }

    /// Return a mutable reference to the wrapped completer.
    pub fn inner_mut(&mut self) -> &mut C {
        &mut self.inner
    }

    /// Consume the wrapper and return the wrapped completer.
    pub fn into_inner(self) -> C {
        self.inner
    }

    fn prepare_suggestions(&self, mut suggestions: Vec<Suggestion>) -> Vec<Suggestion> {
        if self.columns > 1 {
            for suggestion in &mut suggestions {
                suggestion.description = None;
            }
        }

        suggestions
    }
}

impl<C> Completer for HorizontalCompleter<C>
where
    C: Completer,
{
    fn complete(&mut self, line: &str, pos: usize) -> Vec<Suggestion> {
        let suggestions = self.inner.complete(line, pos);
        self.prepare_suggestions(suggestions)
    }
}

#[cfg(test)]
mod tests {
    use reedline::Span;

    use super::*;

    struct FakeCompleter {
        suggestions: Vec<Suggestion>,
    }

    impl Completer for FakeCompleter {
        fn complete(&mut self, _line: &str, _pos: usize) -> Vec<Suggestion> {
            self.suggestions.clone()
        }
    }

    fn suggestion(value: &str, description: &str, span: Span) -> Suggestion {
        Suggestion {
            value: value.to_owned(),
            description: Some(description.to_owned()),
            extra: Some(vec![format!("example for {value}")]),
            span,
            append_whitespace: true,
            style: None,
        }
    }

    #[test]
    fn multi_column_layout_preserves_order_and_removes_descriptions() {
        let span = Span::new(3, 5);
        let suggestions = vec![
            suggestion("alpha", "first", span),
            suggestion("beta", "second", span),
            suggestion("gamma", "third", span),
            suggestion("delta", "fourth", span),
            suggestion("epsilon", "fifth", span),
        ];
        let expected_values: Vec<_> = suggestions
            .iter()
            .map(|suggestion| suggestion.value.clone())
            .collect();
        let mut completer = HorizontalCompleter::new(FakeCompleter { suggestions }, 4);

        let actual = completer.complete("do al", 5);
        let actual_values: Vec<_> = actual
            .iter()
            .map(|suggestion| suggestion.value.clone())
            .collect();

        assert_eq!(actual_values, expected_values);
        assert!(
            actual
                .iter()
                .all(|suggestion| suggestion.description.is_none())
        );
        assert!(actual.iter().all(|suggestion| suggestion.span == span));
        assert!(actual.iter().all(|suggestion| suggestion.append_whitespace));
        assert!(actual.iter().all(|suggestion| suggestion.extra.is_some()));
    }

    #[test]
    fn single_column_layout_keeps_descriptions() {
        let suggestions = vec![suggestion("alpha", "first", Span::new(0, 1))];
        let mut completer = HorizontalCompleter::new(FakeCompleter { suggestions }, 1);

        let actual = completer.complete("a", 1);

        assert_eq!(actual[0].description.as_deref(), Some("first"));
    }

    #[test]
    #[should_panic(expected = "at least one column")]
    fn zero_columns_are_rejected() {
        let _ = HorizontalCompleter::new(
            FakeCompleter {
                suggestions: Vec::new(),
            },
            0,
        );
    }
}
