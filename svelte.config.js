const IGNORED_WARNINGS = [
  "a11y_autofocus",
  "a11y_click_events_have_key_events",
  "a11y_no_static_element_interactions",
  "a11y_no_noninteractive_element_interactions",
  "a11y_interactive_supports_focus",
];

export default {
  compilerOptions: {
    runes: true,
    warningFilter: (warning) => !IGNORED_WARNINGS.includes(warning.code),
  },
};
