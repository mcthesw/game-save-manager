# Preserve Save Unit type across source migration

Status: accepted

A Save Unit's declared file, folder, or registry type remains independent from
whether its location is concrete or expressed as a portable Manifest Path
Pattern. Migrating a legacy concrete path to a pattern preserves the known type;
manifest imports that provide no type may defer to the kind of each resolved
location. Dropping the declared type was rejected because Archive V2 restoration
then has to infer semantics from entry names and shapes that were never intended
to replace the configuration's type authority.
