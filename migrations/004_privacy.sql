-- Undetectable mode is app-wide, so it lives on the single overlay row.
-- Existing databases get the column with privacy off by default.
ALTER TABLE overlay_preferences ADD COLUMN undetectable INTEGER NOT NULL DEFAULT 0;
