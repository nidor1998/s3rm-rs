-- Example Lua event callback for s3rm-rs.
--
-- Define a function named `on_event(event)` that is called for each
-- pipeline event (start, completion, failure, statistics, etc.).
--
-- Available fields on the `event` table:
--   event.event_type            (number)  - Bitflag identifying the event type
--   event.dry_run               (boolean) - Whether the pipeline is in dry-run mode
--   event.key                   (string)  - Object key (nil for non-object events)
--   event.version_id            (string)  - Version ID (nil if not versioned)
--   event.size                  (number)  - Object size in bytes (nil if N/A)
--   event.last_modified         (string)  - ISO-8601 timestamp (nil if N/A)
--   event.e_tag                 (string)  - Entity tag (nil if N/A)
--   event.error_message         (string)  - Error description (nil on success)
--   event.message               (string)  - Human-readable message (nil if N/A)
--   event.stats_deleted_objects (number)  - Total deleted objects (STATS_REPORT only)
--   event.stats_deleted_bytes   (number)  - Total deleted bytes (STATS_REPORT only)
--   event.stats_failed_objects  (number)  - Total failed deletions (STATS_REPORT only)
--   event.stats_skipped_objects (number)  - Total skipped objects (STATS_REPORT only)
--   event.stats_warning_count   (number)  - Warning count (STATS_REPORT only)
--   event.stats_error_count     (number)  - Error count (STATS_REPORT only)
--   event.stats_duration_sec    (number)  - Elapsed seconds (STATS_REPORT only)
--   event.stats_objects_per_sec (number)  - Throughput (STATS_REPORT only)

-- Event type constants (bitflags)
PIPELINE_START  = 2     -- 1 << 1
PIPELINE_END    = 4     -- 1 << 2
DELETE_COMPLETE = 8     -- 1 << 3
DELETE_FAILED   = 16    -- 1 << 4
DELETE_FILTERED = 32    -- 1 << 5
DELETE_WARNING  = 64    -- 1 << 6
PIPELINE_ERROR  = 128   -- 1 << 7
DELETE_CANCEL   = 256   -- 1 << 8
STATS_REPORT    = 512   -- 1 << 9

-- Helper: check if a bitflag is set
local function has_flag(value, flag)
    -- Lua 5.4 bitwise AND
    return (value & flag) ~= 0
end

function on_event(event)
    local et = event.event_type

    if has_flag(et, PIPELINE_START) then
        print("[s3rm] Pipeline started" .. (event.dry_run and " (dry-run)" or ""))
    end

    if has_flag(et, DELETE_COMPLETE) then
        print(string.format("[s3rm] Deleted: %s (%d bytes)",
            event.key or "?", event.size or 0))
    end

    if has_flag(et, DELETE_FAILED) then
        print(string.format("[s3rm] FAILED: %s - %s",
            event.key or "?", event.error_message or "unknown error"))
    end

    if has_flag(et, STATS_REPORT) then
        print(string.format("[s3rm] Progress: %d deleted, %d failed, %.1f obj/s",
            event.stats_deleted_objects or 0,
            event.stats_failed_objects or 0,
            event.stats_objects_per_sec or 0))
    end

    if has_flag(et, PIPELINE_END) then
        print("[s3rm] Pipeline finished")
    end
end
