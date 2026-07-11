# Separate device resources from game bindings

Status: accepted

A Device owns the roots, store accounts, and installations available on that
machine, while each Game stores a Device-keyed binding that selects which of
those resources it uses. Manifest-derived Save Units inherit the Game Device
Binding. A Device-global game selection was rejected as too coarse, and initial
Save-Unit-level overrides were rejected because they duplicate configuration
and leak resolution concerns into every Save Unit.
