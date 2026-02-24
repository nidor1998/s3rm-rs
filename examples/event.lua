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
--   event.stats_error_count     (number)  - Error count (STATS_REPORT only)
--   event.stats_duration_sec    (number)  - Elapsed seconds (STATS_REPORT only)
--   event.stats_objects_per_sec (number)  - Throughput (STATS_REPORT only)

-- Event type constants (bitflags)
PIPELINE_START  = 2     -- 1 << 1
PIPELINE_END    = 4     -- 1 << 2
DELETE_COMPLETE = 8     -- 1 << 3
DELETE_FAILED   = 16    -- 1 << 4
DELETE_FILTERED = 32    -- 1 << 5
PIPELINE_ERROR  = 64    -- 1 << 6
DELETE_CANCEL   = 128   -- 1 << 7
STATS_REPORT    = 256   -- 1 << 8

-- Helper: check if a bitflag is set
local function has_flag(value, flag)
    -- Lua 5.4 bitwise AND
    return (value & flag) ~= 0
end

-- Helper: format an object's common fields into a detail string
local function object_details(event)
    local parts = {}
    if event.key then
        table.insert(parts, "key=" .. event.key)
    end
    if event.version_id then
        table.insert(parts, "version_id=" .. event.version_id)
    end
    if event.size then
        table.insert(parts, "size=" .. tostring(event.size))
    end
    if event.last_modified then
        table.insert(parts, "last_modified=" .. event.last_modified)
    end
    if event.e_tag then
        table.insert(parts, "e_tag=" .. event.e_tag)
    end
    if event.error_message then
        table.insert(parts, "error=" .. event.error_message)
    end
    if event.message then
        table.insert(parts, "message=" .. event.message)
    end
    return table.concat(parts, ", ")
end

function on_event(event)
    local et = event.event_type

    if has_flag(et, PIPELINE_START) then
        print(string.format("[s3rm] Pipeline started (dry_run=%s, event_type=%d)",
            tostring(event.dry_run), et))
        local details = object_details(event)
        if #details > 0 then
            print("        " .. details)
        end
    end

    if has_flag(et, DELETE_COMPLETE) then
        print(string.format("[s3rm] Deleted: %s", object_details(event)))
    end

    if has_flag(et, DELETE_FAILED) then
        print(string.format("[s3rm] FAILED: %s", object_details(event)))
    end

    if has_flag(et, DELETE_FILTERED) then
        print(string.format("[s3rm] Filtered: %s", object_details(event)))
    end

    if has_flag(et, PIPELINE_ERROR) then
        print(string.format("[s3rm] ERROR: %s", object_details(event)))
    end

    if has_flag(et, DELETE_CANCEL) then
        print(string.format("[s3rm] Cancelled: %s", object_details(event)))
    end

    if has_flag(et, STATS_REPORT) then
        print(string.format(
            "[s3rm] Stats: deleted_objects=%d, deleted_bytes=%d, failed=%d, skipped=%d, "
            .. "errors=%d, duration=%.1fs, throughput=%.1f obj/s",
            event.stats_deleted_objects or 0,
            event.stats_deleted_bytes or 0,
            event.stats_failed_objects or 0,
            event.stats_skipped_objects or 0,
            event.stats_error_count or 0,
            event.stats_duration_sec or 0,
            event.stats_objects_per_sec or 0))
    end

    if has_flag(et, PIPELINE_END) then
        print(string.format("[s3rm] Pipeline finished (dry_run=%s)",
            tostring(event.dry_run)))
        local details = object_details(event)
        if #details > 0 then
            print("        " .. details)
        end
    end
end
