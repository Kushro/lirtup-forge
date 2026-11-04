# migrate (round 4, front M): «Move to scaling modes», the migration from 1.x.
migrate-title = Move to scaling modes
migrate-body = { $modes ->
    [0] Your 1.x configuration keeps the model, sampling and effects loose in each profile. They now live in named scaling modes that profiles pick. { $fields ->
        [one] { $fields } setting moves
       *[other] { $fields } settings move
    } to modes that already exist:
    [one] Your 1.x configuration keeps the model, sampling and effects loose in each profile. They now live in named scaling modes that profiles pick. { $fields ->
        [one] { $fields } setting moves
       *[other] { $fields } settings move
    } to { $modes } new mode:
   *[other] Your 1.x configuration keeps the model, sampling and effects loose in each profile. They now live in named scaling modes that profiles pick. { $fields ->
        [one] { $fields } setting moves
       *[other] { $fields } settings move
    } to { $modes } new modes:
}
migrate-new = new
migrate-inherit = { $count ->
    [one] { $profiles } keeps the mode of Default.
   *[other] { $profiles } keep the mode of Default.
}
migrate-backup = The current configuration is copied to { $file } first. Scaling stays as it was.
migrate-legacy-gui = The 1.x tray and palette settings move to Settings too.
migrate-later = Not now
migrate-confirm = Migrate
migrate-done = Configuration migrated. Copy of the old one in { $backup }
migrate-failed = Could not migrate: { $error }
# fx-unify: modes that still use the native effects engine.
migrate-native-title = Update the effects
migrate-native-body = { $count ->
    [one] One mode uses
   *[other] { $count } modes use
} the retired native effects engine: its effects move to the MagpieFX chain, with the same result.
migrate-native-backup = Each mode file that changes is copied to { $file } first.
