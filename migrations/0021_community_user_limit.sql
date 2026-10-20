-- Existing installations retain every user and active state. The Community
-- limit applies to new active users and reactivation, never to historical rows.
CREATE FUNCTION lock_community_user_changes() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    -- Acquire before row locks, matching the administrator mutation API.
    PERFORM pg_advisory_xact_lock(492310718002);
    RETURN NULL;
END $$;
CREATE TRIGGER community_user_change_lock BEFORE INSERT OR UPDATE OR DELETE ON users
    FOR EACH STATEMENT EXECUTE FUNCTION lock_community_user_changes();

CREATE FUNCTION guard_community_user_capacity() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE activates boolean;
BEGIN
    IF TG_OP='INSERT' THEN
        activates := NEW.active;
    ELSE
        activates := NEW.active AND NOT OLD.active;
    END IF;
    IF activates AND (SELECT count(*) FROM users WHERE active)>=10 THEN
        RAISE EXCEPTION 'Haruflow Community supports at most 10 active users, including administrators'
            USING ERRCODE='23514', CONSTRAINT='community_active_user_limit';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER community_user_capacity BEFORE INSERT OR UPDATE ON users
    FOR EACH ROW EXECUTE FUNCTION guard_community_user_capacity();
