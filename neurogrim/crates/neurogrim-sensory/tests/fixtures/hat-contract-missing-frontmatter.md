# Some Hat Without Frontmatter

This file has no YAML frontmatter; it represents a hat catalog entry that has not been
promoted to the structured contract format yet. Per Q4, the sensor treats this case as
"no contract present" rather than as a schema-validation failure (because there is no
schema-target object to validate). Detection of this case is structurally distinct from
detection of an invalid-vocabulary case — the schema-conformance test pins both modes
separately.
