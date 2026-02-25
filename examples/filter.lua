-- Example Lua filter callback for s3rm-rs.
--
-- Define a function named `filter(obj)` that returns true for objects
-- that should be deleted, or false to skip them.
--
-- Available fields on the `obj` table:
--   obj.key              (string)  - Object key (e.g., "logs/2024/access.log")
--   obj.last_modified    (string)  - ISO-8601 timestamp
--   obj.version_id       (string)  - Version ID (nil for unversioned)
--   obj.e_tag            (string)  - Entity tag (MD5 hash)
--   obj.is_latest        (boolean) - Whether this is the latest version
--   obj.is_delete_marker (boolean) - Whether this is a delete marker
--   obj.size             (number)  - Object size in bytes

function filter(obj)
    -- Example 1: Delete objects under the "logs/" prefix
    if string.find(obj.key, "^logs/") then
        return true
    end

    -- Example 2: Delete objects larger than 1 MB
    if obj.size > 1048576 then
        return true
    end

    -- Example 3: Delete objects matching a specific extension
    if string.find(obj.key, "%.tmp$") then
        return true
    end

    -- Skip everything else
    return false
end
