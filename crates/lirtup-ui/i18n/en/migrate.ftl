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
migrate-done = Configuration moved to modes. Copy of the old one in { $backup }
migrate-failed = Could not migrate: { $error }
