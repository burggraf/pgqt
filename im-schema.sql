


SET statement_timeout = 0;
SET lock_timeout = 0;
SET idle_in_transaction_session_timeout = 0;
SET client_encoding = 'UTF8';
SET standard_conforming_strings = on;
SELECT pg_catalog.set_config('search_path', '', false);
SET check_function_bodies = false;
SET xmloption = content;
SET client_min_messages = warning;
SET row_security = off;


CREATE SCHEMA IF NOT EXISTS "public";


ALTER SCHEMA "public" OWNER TO "pg_database_owner";


COMMENT ON SCHEMA "public" IS 'standard public schema';



CREATE TYPE "public"."feedback_category" AS ENUM (
    'bug',
    'feature',
    'question',
    'other'
);


ALTER TYPE "public"."feedback_category" OWNER TO "postgres";


CREATE TYPE "public"."feedback_status" AS ENUM (
    'new',
    'in_progress',
    'resolved',
    'closed'
);


ALTER TYPE "public"."feedback_status" OWNER TO "postgres";


CREATE TYPE "public"."inspection_status" AS ENUM (
    'requested',
    'scheduled',
    'in_progress',
    'completed',
    'cancelled'
);


ALTER TYPE "public"."inspection_status" OWNER TO "postgres";


CREATE TYPE "public"."inspector_selection_mode" AS ENUM (
    'client_chooses',
    'admin_assigns'
);


ALTER TYPE "public"."inspector_selection_mode" OWNER TO "postgres";


CREATE TYPE "public"."invite_status" AS ENUM (
    'pending',
    'accepted',
    'revoked',
    'expired'
);


ALTER TYPE "public"."invite_status" OWNER TO "postgres";


CREATE TYPE "public"."invoice_status" AS ENUM (
    'draft',
    'open',
    'paid',
    'void',
    'uncollectible'
);


ALTER TYPE "public"."invoice_status" OWNER TO "postgres";


CREATE TYPE "public"."platform_permission" AS ENUM (
    'admin',
    'billing_admin',
    'billing_support',
    'edit_inspection_template_samples',
    'edit_report_template_samples',
    'view_feedback',
    'manage_promotions',
    'manage_products',
    'manage_platform_staff'
);


ALTER TYPE "public"."platform_permission" OWNER TO "postgres";


COMMENT ON TYPE "public"."platform_permission" IS 'Platform staff permissions: admin, billing_admin, billing_support, edit_inspection_template_samples, view_feedback, manage_promotions, manage_products';



CREATE TYPE "public"."subscription_status" AS ENUM (
    'trialing',
    'active',
    'past_due',
    'canceled',
    'expired'
);


ALTER TYPE "public"."subscription_status" OWNER TO "postgres";


CREATE TYPE "public"."user_role" AS ENUM (
    'admin',
    'inspector',
    'client'
);


ALTER TYPE "public"."user_role" OWNER TO "postgres";


CREATE OR REPLACE FUNCTION "public"."accept_client_invitation"("p_token" "text", "p_full_name" "text" DEFAULT NULL::"text") RETURNS json
    LANGUAGE "plpgsql" SECURITY DEFINER
    SET "search_path" TO 'public'
    AS $$
DECLARE
  v_invitation client_invitations;
  v_user_id uuid;
  v_company_name text;
  v_client_name text;
BEGIN
  v_user_id := auth.uid();

  IF v_user_id IS NULL THEN
    RAISE EXCEPTION 'Must be authenticated to accept invitation';
  END IF;

  -- Get and validate invitation
  SELECT * INTO v_invitation
  FROM client_invitations
  WHERE token = p_token;

  IF v_invitation IS NULL THEN
    RAISE EXCEPTION 'Invalid invitation token';
  END IF;

  IF v_invitation.status != 'pending' THEN
    RAISE EXCEPTION 'Invitation is no longer valid (status: %)', v_invitation.status;
  END IF;

  IF v_invitation.expires_at < now() THEN
    UPDATE client_invitations SET status = 'expired' WHERE id = v_invitation.id;
    RAISE EXCEPTION 'Invitation has expired';
  END IF;

  -- Get client name for user profile
  SELECT name INTO v_client_name FROM clients WHERE id = v_invitation.client_id;

  -- Create user profile with role='client'
  INSERT INTO users (id, email, full_name, current_company_id, company_id, role)
  VALUES (
    v_user_id,
    v_invitation.email,
    COALESCE(p_full_name, v_client_name, split_part(v_invitation.email, '@', 1)),
    v_invitation.company_id,
    v_invitation.company_id,
    'client'
  );

  -- Create company membership
  INSERT INTO company_memberships (user_id, company_id, role)
  VALUES (v_user_id, v_invitation.company_id, 'client');

  -- Link client record to user and set status to active
  UPDATE clients
  SET user_id = v_user_id, status = 'active'
  WHERE id = v_invitation.client_id
    AND company_id = v_invitation.company_id;

  -- Mark invitation as accepted
  UPDATE client_invitations
  SET status = 'accepted', updated_at = now()
  WHERE id = v_invitation.id;

  -- Get company name for response
  SELECT name INTO v_company_name FROM companies WHERE id = v_invitation.company_id;

  RETURN json_build_object(
    'success', true,
    'company_id', v_invitation.company_id,
    'company_name', v_company_name,
    'client_id', v_invitation.client_id
  );
END;
$$;


ALTER FUNCTION "public"."accept_client_invitation"("p_token" "text", "p_full_name" "text") OWNER TO "postgres";


CREATE OR REPLACE FUNCTION "public"."accept_client_invitation"("p_token" "text", "p_user_id" "uuid", "p_full_name" "text" DEFAULT NULL::"text") RETURNS json
    LANGUAGE "plpgsql" SECURITY DEFINER
    SET "search_path" TO 'public'
    AS $$
DECLARE
  v_invitation record;
  v_client_name text;
  v_existing_user_id uuid;
BEGIN
  -- Get the invitation
  SELECT ci.*, c.name as company_name
  INTO v_invitation
  FROM client_invitations ci
  JOIN companies c ON c.id = ci.company_id
  WHERE ci.token = p_token;

  IF NOT FOUND THEN
    RETURN json_build_object('success', false, 'error', 'Invalid invitation token');
  END IF;

  -- Check invitation status - allow if pending OR if already accepted by same user (retry case)
  IF v_invitation.status = 'accepted' THEN
    -- Check if this client already has the user linked (successful previous attempt)
    SELECT user_id INTO v_existing_user_id FROM clients WHERE id = v_invitation.client_id;
    IF v_existing_user_id IS NOT NULL THEN
      -- Already completed successfully, return success for idempotency
      RETURN json_build_object('success', true, 'message', 'Already accepted');
    END IF;
  ELSIF v_invitation.status != 'pending' THEN
    RETURN json_build_object('success', false, 'error', 'Invitation is no longer valid');
  END IF;

  -- Check expiration (skip for accepted invitations in retry case)
  IF v_invitation.status = 'pending' AND v_invitation.expires_at < now() THEN
    RETURN json_build_object('success', false, 'error', 'Invitation has expired');
  END IF;

  -- Get client name from the database (don't trust frontend value)
  SELECT name INTO v_client_name FROM clients WHERE id = v_invitation.client_id;

  -- Create or update user profile (idempotent)
  -- Always use the client name from the database, not the passed parameter
  INSERT INTO users (id, email, full_name, company_id, current_company_id, role)
  VALUES (
    p_user_id,
    v_invitation.email,
    COALESCE(v_client_name, 'Client'),
    v_invitation.company_id,
    v_invitation.company_id,
    'client'
  )
  ON CONFLICT (id) DO UPDATE SET
    current_company_id = EXCLUDED.current_company_id,
    full_name = COALESCE(v_client_name, users.full_name),  -- Update name if we have it
    role = 'client';

  -- Create company membership (idempotent)
  INSERT INTO company_memberships (user_id, company_id, role)
  VALUES (
    p_user_id,
    v_invitation.company_id,
    'client'
  )
  ON CONFLICT (user_id, company_id) DO UPDATE SET
    role = 'client';

  -- Update client record to link user
  UPDATE clients
  SET user_id = p_user_id, status = 'active'
  WHERE id = v_invitation.client_id;

  -- Mark invitation as accepted
  UPDATE client_invitations
  SET status = 'accepted'
  WHERE id = v_invitation.id;

  RETURN json_build_object('success', true);
END;
$$;


ALTER FUNCTION "public"."accept_client_invitation"("p_token" "text", "p_user_id" "uuid", "p_full_name" "text") OWNER TO "postgres";


CREATE OR REPLACE FUNCTION "public"."accept_client_invitation"("p_token" "text", "p_user_id" "uuid", "p_full_name" "text" DEFAULT NULL::"text", "p_terms_accepted" boolean DEFAULT false) RETURNS json
    LANGUAGE "plpgsql" SECURITY DEFINER
    SET "search_path" TO 'public'
    AS $$
DECLARE
  v_invitation record;
  v_client_name text;
  v_existing_user_id uuid;
BEGIN
  -- Get the invitation
  SELECT ci.*, c.name as company_name
  INTO v_invitation
  FROM client_invitations ci
  JOIN companies c ON c.id = ci.company_id
  WHERE ci.token = p_token;

  IF NOT FOUND THEN
    RETURN json_build_object('success', false, 'error', 'Invalid invitation token');
  END IF;

  -- Check invitation status - allow if pending OR if already accepted by same user (retry case)
  IF v_invitation.status = 'accepted' THEN
    -- Check if this client already has the user linked (successful previous attempt)
    SELECT user_id INTO v_existing_user_id FROM clients WHERE id = v_invitation.client_id;
    IF v_existing_user_id IS NOT NULL THEN
      -- Already completed successfully, return success for idempotency
      RETURN json_build_object('success', true, 'message', 'Already accepted');
    END IF;
  ELSIF v_invitation.status != 'pending' THEN
    RETURN json_build_object('success', false, 'error', 'Invitation is no longer valid');
  END IF;

  -- Check expiration (skip for accepted invitations in retry case)
  IF v_invitation.status = 'pending' AND v_invitation.expires_at < now() THEN
    RETURN json_build_object('success', false, 'error', 'Invitation has expired');
  END IF;

  -- Require terms acceptance for new accounts (skip for retry/idempotent cases)
  IF v_invitation.status = 'pending' AND NOT p_terms_accepted THEN
    RETURN json_build_object(
      'success', false,
      'error', 'Terms and conditions must be accepted to create an account'
    );
  END IF;

  -- Get client name from the database (don't trust frontend value)
  SELECT name INTO v_client_name FROM clients WHERE id = v_invitation.client_id;

  -- Create or update user profile (idempotent)
  -- Always use the client name from the database, not the passed parameter
  INSERT INTO users (id, email, full_name, company_id, current_company_id, role, terms_accepted_at)
  VALUES (
    p_user_id,
    v_invitation.email,
    COALESCE(v_client_name, 'Client'),
    v_invitation.company_id,
    v_invitation.company_id,
    'client',
    NOW()
  )
  ON CONFLICT (id) DO UPDATE SET
    current_company_id = EXCLUDED.current_company_id,
    full_name = COALESCE(v_client_name, users.full_name),  -- Update name if we have it
    role = 'client',
    terms_accepted_at = COALESCE(users.terms_accepted_at, NOW()); -- Only set if not already set

  -- Create company membership (idempotent)
  INSERT INTO company_memberships (user_id, company_id, role)
  VALUES (
    p_user_id,
    v_invitation.company_id,
    'client'
  )
  ON CONFLICT (user_id, company_id) DO UPDATE SET
    role = 'client';

  -- Update client record to link user
  UPDATE clients
  SET user_id = p_user_id, status = 'active'
  WHERE id = v_invitation.client_id;

  -- Mark invitation as accepted
  UPDATE client_invitations
  SET status = 'accepted'
  WHERE id = v_invitation.id;

  RETURN json_build_object('success', true);
END;
$$;


ALTER FUNCTION "public"."accept_client_invitation"("p_token" "text", "p_user_id" "uuid", "p_full_name" "text", "p_terms_accepted" boolean) OWNER TO "postgres";


CREATE OR REPLACE FUNCTION "public"."accept_invitation"("p_token" "text", "p_full_name" "text" DEFAULT NULL::"text") RETURNS json
    LANGUAGE "plpgsql" SECURITY DEFINER
    SET "search_path" TO 'public'
    AS $$
DECLARE
  v_invitation invitations;
  v_user_id uuid;
  v_existing_user users;
  v_company_name text;
BEGIN
  v_user_id := auth.uid();

  IF v_user_id IS NULL THEN
    RAISE EXCEPTION 'Must be authenticated to accept invitation';
  END IF;

  -- Get and validate invitation
  SELECT * INTO v_invitation
  FROM invitations
  WHERE token = p_token;

  IF v_invitation IS NULL THEN
    RAISE EXCEPTION 'Invalid invitation token';
  END IF;

  IF v_invitation.status != 'pending' THEN
    RAISE EXCEPTION 'Invitation is no longer valid (status: %)', v_invitation.status;
  END IF;

  IF v_invitation.expires_at < now() THEN
    UPDATE invitations SET status = 'expired' WHERE id = v_invitation.id;
    RAISE EXCEPTION 'Invitation has expired';
  END IF;

  -- Check if user profile exists
  SELECT * INTO v_existing_user FROM users WHERE id = v_user_id;

  IF v_existing_user IS NULL THEN
    -- Create user profile (new user accepting invite)
    INSERT INTO users (id, email, full_name, current_company_id, company_id, role)
    VALUES (
      v_user_id,
      v_invitation.email,
      COALESCE(p_full_name, split_part(v_invitation.email, '@', 1)),
      v_invitation.company_id,
      v_invitation.company_id,
      v_invitation.role
    );
  END IF;

  -- Check if already a member
  IF EXISTS (
    SELECT 1 FROM company_memberships
    WHERE user_id = v_user_id AND company_id = v_invitation.company_id
  ) THEN
    RAISE EXCEPTION 'Already a member of this company';
  END IF;

  -- Create membership
  INSERT INTO company_memberships (user_id, company_id, role)
  VALUES (v_user_id, v_invitation.company_id, v_invitation.role);

  -- Create default availability for admin/inspector roles (not for clients)
  IF v_invitation.role IN ('admin', 'inspector') THEN
    PERFORM insert_default_availability(v_user_id, v_invitation.company_id);
  END IF;

  -- Update user's current company to the new one
  UPDATE users
  SET current_company_id = v_invitation.company_id
  WHERE id = v_user_id;

  -- Mark invitation accepted
  UPDATE invitations
  SET status = 'accepted', accepted_at = now()
  WHERE id = v_invitation.id;

  -- Get company name for response
  SELECT name INTO v_company_name FROM companies WHERE id = v_invitation.company_id;

  RETURN json_build_object(
    'success', true,
    'company_id', v_invitation.company_id,
    'company_name', v_company_name,
    'role', v_invitation.role
  );
END;
$$;


ALTER FUNCTION "public"."accept_invitation"("p_token" "text", "p_full_name" "text") OWNER TO "postgres";


CREATE OR REPLACE FUNCTION "public"."accept_terms"() RETURNS json
    LANGUAGE "plpgsql" SECURITY DEFINER
    SET "search_path" TO 'public'
    AS $$
  BEGIN
    UPDATE users
    SET terms_accepted_at = NOW()
    WHERE id = auth.uid();

    IF NOT FOUND THEN
      RETURN json_build_object('success', false, 'error',
  'User not found');
    END IF;

    RETURN json_build_object('success', true);
  END;
  $$;


ALTER FUNCTION "public"."accept_terms"() OWNER TO "postgres";


CREATE OR REPLACE FUNCTION "public"."assign_inspector"("p_inspection_id" "uuid", "p_inspector_id" "uuid") RETURNS json
    LANGUAGE "plpgsql" SECURITY DEFINER
    SET "search_path" TO 'public'
    AS $$
DECLARE
  v_inspection inspections;
  v_user_company_id uuid;
BEGIN
  v_user_company_id := get_user_company_id();

  IF v_user_company_id IS NULL THEN
    RAISE EXCEPTION 'No active company context';
  END IF;

  IF NOT is_admin() THEN
    RAISE EXCEPTION 'Only admins can assign inspectors';
  END IF;

  SELECT * INTO v_inspection
  FROM inspections
  WHERE id = p_inspection_id AND company_id = v_user_company_id;

  IF v_inspection IS NULL THEN
    RAISE EXCEPTION 'Inspection not found';
  END IF;

  IF NOT EXISTS (
    SELECT 1 FROM company_memberships
    WHERE user_id = p_inspector_id
      AND company_id = v_user_company_id
      AND role IN ('admin', 'inspector')
  ) THEN
    RAISE EXCEPTION 'Inspector not found in company';
  END IF;

  UPDATE inspections
  SET
    inspector_id = p_inspector_id,
    status = CASE
      WHEN status = 'requested' THEN 'scheduled'::inspection_status
      ELSE status
    END,
    updated_at = now()
  WHERE id = p_inspection_id;

  RETURN json_build_object(
    'success', true,
    'inspection_id', p_inspection_id,
    'inspector_id', p_inspector_id
  );
END;
$$;


ALTER FUNCTION "public"."assign_inspector"("p_inspection_id" "uuid", "p_inspector_id" "uuid") OWNER TO "postgres";


CREATE OR REPLACE FUNCTION "public"."can_access_inspection_data_storage"("object_path" "text") RETURNS boolean
    LANGUAGE "plpgsql" SECURITY DEFINER
    SET "search_path" TO 'public'
    AS $$
DECLARE
  path_parts TEXT[];
  path_company_id UUID;
  path_inspection_id UUID;
  user_company_id UUID;
  user_role TEXT;
  inspection_client_id UUID;
  client_user_id UUID;
BEGIN
  path_parts := string_to_array(object_path, '/');

  IF array_length(path_parts, 1) < 2 THEN
    RETURN FALSE;
  END IF;

  BEGIN
    path_company_id := path_parts[1]::UUID;
    path_inspection_id := path_parts[2]::UUID;
  EXCEPTION WHEN OTHERS THEN
    RETURN FALSE;
  END;

  SELECT u.company_id, u.role::TEXT INTO user_company_id, user_role
  FROM public.users u
  WHERE u.id = auth.uid();

  IF user_company_id IS NULL OR path_company_id != user_company_id THEN
    RETURN FALSE;
  END IF;

  -- Staff can access all company inspection data
  IF user_role IN ('admin', 'inspector') THEN
    RETURN TRUE;
  END IF;

  -- Clients can only access their own inspection data
  IF user_role = 'client' THEN
    SELECT i.client_id INTO inspection_client_id
    FROM public.inspections i
    WHERE i.id = path_inspection_id AND i.company_id = user_company_id;

    IF inspection_client_id IS NULL THEN
      RETURN FALSE;
    END IF;

    SELECT c.user_id INTO client_user_id
    FROM public.clients c
    WHERE c.id = inspection_client_id;

    RETURN client_user_id = auth.uid();
  END IF;

  RETURN FALSE;
END;
$$;


ALTER FUNCTION "public"."can_access_inspection_data_storage"("object_path" "text") OWNER TO "postgres";


CREATE OR REPLACE FUNCTION "public"."can_access_inspection_media"("object_path" "text") RETURNS boolean
    LANGUAGE "plpgsql" SECURITY DEFINER
    SET "search_path" TO 'public'
    AS $$
DECLARE
  path_parts TEXT[];
  path_company_id UUID;
  path_inspection_id UUID;
  user_company_id UUID;
  user_role TEXT;
  inspection_client_id UUID;
  client_user_id UUID;
BEGIN
  -- Parse path: company_id/inspection_id/...
  path_parts := string_to_array(object_path, '/');
  
  IF array_length(path_parts, 1) < 2 THEN
    RETURN FALSE;
  END IF;
  
  BEGIN
    path_company_id := path_parts[1]::UUID;
    path_inspection_id := path_parts[2]::UUID;
  EXCEPTION WHEN OTHERS THEN
    RETURN FALSE;
  END;
  
  -- Get user's company and role
  SELECT u.company_id, u.role::TEXT INTO user_company_id, user_role
  FROM public.users u
  WHERE u.id = auth.uid();
  
  IF user_company_id IS NULL THEN
    RETURN FALSE;
  END IF;
  
  -- Must be same company
  IF path_company_id != user_company_id THEN
    RETURN FALSE;
  END IF;
  
  -- Admin and inspector can access all company media
  IF user_role IN ('admin', 'inspector') THEN
    RETURN TRUE;
  END IF;
  
  -- Client can only access their own inspection's media
  IF user_role = 'client' THEN
    SELECT i.client_id INTO inspection_client_id
    FROM public.inspections i
    WHERE i.id = path_inspection_id AND i.company_id = user_company_id;
    
    IF inspection_client_id IS NULL THEN
      RETURN FALSE;
    END IF;
    
    -- Check if client record is linked to this user
    SELECT c.user_id INTO client_user_id
    FROM public.clients c
    WHERE c.id = inspection_client_id;
    
    RETURN client_user_id = auth.uid();
  END IF;
  
  RETURN FALSE;
END;
$$;


ALTER FUNCTION "public"."can_access_inspection_media"("object_path" "text") OWNER TO "postgres";


CREATE OR REPLACE FUNCTION "public"."can_access_report_storage"("object_path" "text") RETURNS boolean
    LANGUAGE "plpgsql" SECURITY DEFINER
    SET "search_path" TO 'public'
    AS $$
DECLARE
  path_parts TEXT[];
  path_company_id UUID;
  path_inspection_id UUID;
  user_company_id UUID;
  user_role TEXT;
  inspection_client_id UUID;
  client_user_id UUID;
BEGIN
  -- Parse path: company_id/inspection_id/report.pdf
  path_parts := string_to_array(object_path, '/');

  IF array_length(path_parts, 1) < 2 THEN
    RETURN FALSE;
  END IF;

  BEGIN
    path_company_id := path_parts[1]::UUID;
    path_inspection_id := path_parts[2]::UUID;
  EXCEPTION WHEN OTHERS THEN
    RETURN FALSE;
  END;

  -- Get current user's company and role
  SELECT u.company_id, u.role::TEXT INTO user_company_id, user_role
  FROM public.users u
  WHERE u.id = auth.uid();

  IF user_company_id IS NULL THEN
    RETURN FALSE;
  END IF;

  -- Must be same company
  IF path_company_id != user_company_id THEN
    RETURN FALSE;
  END IF;

  -- Staff (admin/inspector) can access all company reports
  IF user_role IN ('admin', 'inspector') THEN
    RETURN TRUE;
  END IF;

  -- Clients can only access reports for their own inspections
  IF user_role = 'client' THEN
    SELECT i.client_id INTO inspection_client_id
    FROM public.inspections i
    WHERE i.id = path_inspection_id AND i.company_id = path_company_id;

    IF inspection_client_id IS NULL THEN
      RETURN FALSE;
    END IF;

    SELECT c.user_id INTO client_user_id
    FROM public.clients c
    WHERE c.id = inspection_client_id;

    RETURN client_user_id = auth.uid();
  END IF;

  RETURN FALSE;
END;
$$;


ALTER FUNCTION "public"."can_access_report_storage"("object_path" "text") OWNER TO "postgres";


CREATE OR REPLACE FUNCTION "public"."can_access_template_storage"("object_path" "text") RETURNS boolean
    LANGUAGE "plpgsql" SECURITY DEFINER
    SET "search_path" TO 'public'
    AS $$
DECLARE
  path_parts TEXT[];
  path_company_id UUID;
  user_company_id UUID;
BEGIN
  path_parts := string_to_array(object_path, '/');

  IF array_length(path_parts, 1) < 1 THEN
    RETURN FALSE;
  END IF;

  BEGIN
    path_company_id := path_parts[1]::UUID;
  EXCEPTION WHEN OTHERS THEN
    RETURN FALSE;
  END;

  SELECT u.company_id INTO user_company_id
  FROM public.users u
  WHERE u.id = auth.uid();

  RETURN user_company_id = path_company_id;
END;
$$;


ALTER FUNCTION "public"."can_access_template_storage"("object_path" "text") OWNER TO "postgres";


CREATE OR REPLACE FUNCTION "public"."can_upload_company_logo"("file_path" "text") RETURNS boolean
    LANGUAGE "plpgsql" SECURITY DEFINER
    SET "search_path" TO 'public'
    AS $$
DECLARE
  path_parts text[];
  company_uuid uuid;
  user_role text;
BEGIN
  -- Split the path into parts
  path_parts := string_to_array(file_path, '/');

  -- Check path structure: companies/{uuid}/logo.jpg
  IF array_length(path_parts, 1) < 2 OR path_parts[1] != 'companies' THEN
    RETURN false;
  END IF;

  -- Try to parse the company ID
  BEGIN
    company_uuid := path_parts[2]::uuid;
  EXCEPTION WHEN OTHERS THEN
    RETURN false;
  END;

  -- Check if user is an admin of this company
  SELECT cm.role INTO user_role
  FROM company_memberships cm
  WHERE cm.user_id = auth.uid()
    AND cm.company_id = company_uuid;

  RETURN user_role = 'admin';
END;
$$;


ALTER FUNCTION "public"."can_upload_company_logo"("file_path" "text") OWNER TO "postgres";


CREATE OR REPLACE FUNCTION "public"."can_upload_inspection_media"("object_path" "text") RETURNS boolean
    LANGUAGE "plpgsql" SECURITY DEFINER
    SET "search_path" TO 'public'
    AS $$
DECLARE
  path_parts TEXT[];
  path_company_id UUID;
  user_company_id UUID;
  user_role TEXT;
BEGIN
  -- Parse path: company_id/inspection_id/...
  path_parts := string_to_array(object_path, '/');
  
  IF array_length(path_parts, 1) < 2 THEN
    RETURN FALSE;
  END IF;
  
  BEGIN
    path_company_id := path_parts[1]::UUID;
  EXCEPTION WHEN OTHERS THEN
    RETURN FALSE;
  END;
  
  -- Get user's company and role
  SELECT u.company_id, u.role::TEXT INTO user_company_id, user_role
  FROM public.users u
  WHERE u.id = auth.uid();
  
  -- Must be same company and staff role
  RETURN user_company_id = path_company_id AND user_role IN ('admin', 'inspector');
END;
$$;


ALTER FUNCTION "public"."can_upload_inspection_media"("object_path" "text") OWNER TO "postgres";


CREATE OR REPLACE FUNCTION "public"."can_write_inspection_data_storage"("object_path" "text") RETURNS boolean
    LANGUAGE "plpgsql" SECURITY DEFINER
    SET "search_path" TO 'public'
    AS $$
DECLARE
  path_parts TEXT[];
  path_company_id UUID;
  user_company_id UUID;
  user_role TEXT;
BEGIN
  path_parts := string_to_array(object_path, '/');

  IF array_length(path_parts, 1) < 2 THEN
    RETURN FALSE;
  END IF;

  BEGIN
    path_company_id := path_parts[1]::UUID;
  EXCEPTION WHEN OTHERS THEN
    RETURN FALSE;
  END;

  SELECT u.company_id, u.role::TEXT INTO user_company_id, user_role
  FROM public.users u
  WHERE u.id = auth.uid();

  RETURN user_company_id = path_company_id AND user_role IN ('admin', 'inspector');
END;
$$;


ALTER FUNCTION "public"."can_write_inspection_data_storage"("object_path" "text") OWNER TO "postgres";


CREATE OR REPLACE FUNCTION "public"."can_write_report_storage"("object_path" "text") RETURNS boolean
    LANGUAGE "plpgsql" SECURITY DEFINER
    SET "search_path" TO 'public'
    AS $$
DECLARE
  path_parts TEXT[];
  path_company_id UUID;
  user_company_id UUID;
  user_role TEXT;
BEGIN
  path_parts := string_to_array(object_path, '/');

  IF array_length(path_parts, 1) < 3 THEN
    RETURN FALSE;
  END IF;

  BEGIN
    path_company_id := path_parts[1]::UUID;
  EXCEPTION WHEN OTHERS THEN
    RETURN FALSE;
  END;

  SELECT u.company_id, u.role::TEXT INTO user_company_id, user_role
  FROM public.users u
  WHERE u.id = auth.uid();

  RETURN user_company_id = path_company_id AND user_role IN ('admin', 'inspector');
END;
$$;


ALTER FUNCTION "public"."can_write_report_storage"("object_path" "text") OWNER TO "postgres";


CREATE OR REPLACE FUNCTION "public"."can_write_template_storage"("object_path" "text") RETURNS boolean
    LANGUAGE "plpgsql" SECURITY DEFINER
    SET "search_path" TO 'public'
    AS $$
DECLARE
  path_parts TEXT[];
  path_company_id UUID;
  user_company_id UUID;
  user_role TEXT;
BEGIN
  path_parts := string_to_array(object_path, '/');

  IF array_length(path_parts, 1) < 1 THEN
    RETURN FALSE;
  END IF;

  BEGIN
    path_company_id := path_parts[1]::UUID;
  EXCEPTION WHEN OTHERS THEN
    RETURN FALSE;
  END;

  SELECT u.company_id, u.role::TEXT INTO user_company_id, user_role
  FROM public.users u
  WHERE u.id = auth.uid();

  RETURN user_company_id = path_company_id AND user_role = 'admin';
END;
$$;


ALTER FUNCTION "public"."can_write_template_storage"("object_path" "text") OWNER TO "postgres";


CREATE OR REPLACE FUNCTION "public"."count_template_structure"("structure" "jsonb") RETURNS TABLE("sections" integer, "fields" integer)
    LANGUAGE "plpgsql"
    AS $$
DECLARE
  section_count integer := 0;
  field_count integer := 0;
BEGIN
  -- Recursive CTE to count all sections and their fields
  WITH RECURSIVE section_tree AS (
    -- Base case: top-level sections
    SELECT
      value as section,
      1 as depth
    FROM jsonb_array_elements(structure->'sections')

    UNION ALL

    -- Recursive case: nested sections
    SELECT
      nested.value as section,
      st.depth + 1
    FROM section_tree st,
         jsonb_array_elements(st.section->'sections') as nested
    WHERE st.section->'sections' IS NOT NULL
      AND jsonb_array_length(st.section->'sections') > 0
  )
  SELECT
    COUNT(*)::integer,
    COALESCE(SUM(jsonb_array_length(section->'fields'))::integer, 0)
  INTO section_count, field_count
  FROM section_tree;

  RETURN QUERY SELECT section_count, field_count;
END;
$$;


ALTER FUNCTION "public"."count_template_structure"("structure" "jsonb") OWNER TO "postgres";


CREATE OR REPLACE FUNCTION "public"."create_booking"("p_company_slug" "text", "p_scheduled_date" timestamp with time zone, "p_duration_minutes" integer, "p_inspector_id" "uuid", "p_property_address" "text", "p_client_name" "text", "p_client_email" "text", "p_client_phone" "text", "p_client_type" "text" DEFAULT 'homeowner'::"text", "p_notes" "text" DEFAULT NULL::"text") RETURNS "uuid"
    LANGUAGE "plpgsql" SECURITY DEFINER
    SET "search_path" TO 'public'
    AS $$
DECLARE
  v_company_id UUID;
  v_client_id UUID;
  v_inspection_id UUID;
  v_requires_confirmation BOOLEAN;
  v_status TEXT;
BEGIN
  SELECT id, booking_requires_confirmation INTO v_company_id, v_requires_confirmation
  FROM companies WHERE slug = p_company_slug;

  IF v_company_id IS NULL THEN
    RAISE EXCEPTION 'Company not found';
  END IF;

  SELECT id INTO v_client_id
  FROM clients
  WHERE company_id = v_company_id AND email = p_client_email;

  IF v_client_id IS NULL THEN
    INSERT INTO clients (company_id, name, email, phone, client_type, status)
    VALUES (v_company_id, p_client_name, p_client_email, p_client_phone, p_client_type, 'prospect')
    RETURNING id INTO v_client_id;
  ELSE
    UPDATE clients SET
      name = COALESCE(p_client_name, name),
      phone = COALESCE(p_client_phone, phone)
    WHERE id = v_client_id;
  END IF;

  IF p_inspector_id IS NULL OR v_requires_confirmation THEN
    v_status := 'requested';
  ELSE
    v_status := 'scheduled';
  END IF;

  INSERT INTO inspections (
    company_id, client_id, inspector_id, property_address,
    scheduled_date, duration_minutes, status, notes
  ) VALUES (
    v_company_id, v_client_id, p_inspector_id, p_property_address,
    p_scheduled_date, p_duration_minutes, v_status::inspection_status, p_notes
  )
  RETURNING id INTO v_inspection_id;

  RETURN v_inspection_id;
END;
$$;


ALTER FUNCTION "public"."create_booking"("p_company_slug" "text", "p_scheduled_date" timestamp with time zone, "p_duration_minutes" integer, "p_inspector_id" "uuid", "p_property_address" "text", "p_client_name" "text", "p_client_email" "text", "p_client_phone" "text", "p_client_type" "text", "p_notes" "text") OWNER TO "postgres";


CREATE OR REPLACE FUNCTION "public"."create_company_and_user"("user_id" "uuid", "user_email" "text", "user_full_name" "text", "company_name" "text") RETURNS json
    LANGUAGE "plpgsql" SECURITY DEFINER
    SET "search_path" TO 'public'
    AS $$
DECLARE
  new_company_id uuid;
  company_slug text;
  existing_user users;
BEGIN
  -- Check if user already exists
  SELECT * INTO existing_user FROM users WHERE id = user_id;

  IF existing_user IS NOT NULL THEN
    -- User already has a profile, return their current company
    RETURN json_build_object(
      'company_id', existing_user.current_company_id,
      'success', true,
      'already_exists', true
    );
  END IF;

  -- Generate slug from company name (lowercase, replace non-alphanumeric with hyphens)
  company_slug := LOWER(REGEXP_REPLACE(company_name, '[^a-zA-Z0-9]', '-', 'g'));

  -- Ensure slug uniqueness by appending random suffix if needed
  WHILE EXISTS (SELECT 1 FROM companies WHERE slug = company_slug) LOOP
    company_slug := company_slug || '-' || SUBSTRING(gen_random_uuid()::text FROM 1 FOR 8);
  END LOOP;

  -- Create the company with slug
  INSERT INTO companies (name, slug)
  VALUES (company_name, company_slug)
  RETURNING id INTO new_company_id;

  -- Create the user profile with current_company_id
  INSERT INTO users (id, email, full_name, current_company_id, company_id, role)
  VALUES (user_id, user_email, user_full_name, new_company_id, new_company_id, 'admin');

  -- Create membership (admin of their own company)
  INSERT INTO company_memberships (user_id, company_id, role)
  VALUES (user_id, new_company_id, 'admin');

  -- Create default availability (Mon-Fri 8am-5pm)
  PERFORM insert_default_availability(user_id, new_company_id);

  RETURN json_build_object(
    'company_id', new_company_id,
    'success', true
  );
END;
$$;


ALTER FUNCTION "public"."create_company_and_user"("user_id" "uuid", "user_email" "text", "user_full_name" "text", "company_name" "text") OWNER TO "postgres";


CREATE OR REPLACE FUNCTION "public"."create_company_and_user"("user_id" "uuid", "user_email" "text", "user_full_name" "text", "company_name" "text", "p_terms_accepted" boolean DEFAULT false) RETURNS json
    LANGUAGE "plpgsql" SECURITY DEFINER
    SET "search_path" TO 'public'
    AS $$
DECLARE
  new_company_id uuid;
  company_slug text;
  existing_user users;
BEGIN
  -- Require terms acceptance for new accounts
  IF NOT p_terms_accepted THEN
    RETURN json_build_object(
      'success', false,
      'error', 'Terms and conditions must be accepted to create an account'
    );
  END IF;

  -- Check if user already exists
  SELECT * INTO existing_user FROM users WHERE id = user_id;

  IF existing_user IS NOT NULL THEN
    -- User already has a profile, return their current company
    RETURN json_build_object(
      'company_id', existing_user.current_company_id,
      'success', true,
      'already_exists', true
    );
  END IF;

  -- Generate slug from company name (lowercase, replace non-alphanumeric with hyphens)
  company_slug := LOWER(REGEXP_REPLACE(company_name, '[^a-zA-Z0-9]', '-', 'g'));

  -- Ensure slug uniqueness by appending random suffix if needed
  WHILE EXISTS (SELECT 1 FROM companies WHERE slug = company_slug) LOOP
    company_slug := company_slug || '-' || SUBSTRING(gen_random_uuid()::text FROM 1 FOR 8);
  END LOOP;

  -- Create the company with slug
  INSERT INTO companies (name, slug)
  VALUES (company_name, company_slug)
  RETURNING id INTO new_company_id;

  -- Create the user profile with current_company_id and terms acceptance
  INSERT INTO users (id, email, full_name, current_company_id, company_id, role, terms_accepted_at)
  VALUES (user_id, user_email, user_full_name, new_company_id, new_company_id, 'admin', NOW());

  -- Create membership (admin of their own company)
  INSERT INTO company_memberships (user_id, company_id, role)
  VALUES (user_id, new_company_id, 'admin');

  -- Create default availability (Mon-Fri 8am-5pm)
  PERFORM insert_default_availability(user_id, new_company_id);

  RETURN json_build_object(
    'company_id', new_company_id,
    'success', true
  );
END;
$$;


ALTER FUNCTION "public"."create_company_and_user"("user_id" "uuid", "user_email" "text", "user_full_name" "text", "company_name" "text", "p_terms_accepted" boolean) OWNER TO "postgres";


CREATE OR REPLACE FUNCTION "public"."create_invitation"("p_email" "text", "p_role" "public"."user_role" DEFAULT 'inspector'::"public"."user_role") RETURNS json
    LANGUAGE "plpgsql" SECURITY DEFINER
    SET "search_path" TO 'public'
    AS $$
DECLARE
  v_company_id uuid;
  v_token text;
  v_invitation invitations;
  v_company_name text;
  v_inviter_name text;
BEGIN
  v_company_id := get_user_company_id();
  IF v_company_id IS NULL THEN
    RAISE EXCEPTION 'No active company context';
  END IF;
  IF NOT is_admin() THEN
    RAISE EXCEPTION 'Only admins can invite members';
  END IF;
  IF EXISTS (
    SELECT 1 FROM invitations
    WHERE email = p_email AND company_id = v_company_id AND status = 'pending' AND expires_at > now()
  ) THEN
    RAISE EXCEPTION 'Pending invitation already exists for this email';
  END IF;
  IF EXISTS (
    SELECT 1 FROM company_memberships cm
    JOIN users u ON u.id = cm.user_id
    WHERE u.email = p_email AND cm.company_id = v_company_id
  ) THEN
    RAISE EXCEPTION 'User is already a member of this company';
  END IF;
  -- Use extensions.gen_random_bytes with schema prefix
  v_token := encode(extensions.gen_random_bytes(32), 'hex');
  SELECT name INTO v_company_name FROM companies WHERE id = v_company_id;
  SELECT full_name INTO v_inviter_name FROM users WHERE id = auth.uid();
  INSERT INTO invitations (company_id, email, role, token, invited_by)
  VALUES (v_company_id, p_email, p_role, v_token, auth.uid())
  RETURNING * INTO v_invitation;
  RETURN json_build_object(
    'id', v_invitation.id, 'token', v_token, 'email', p_email, 'role', p_role,
    'expires_at', v_invitation.expires_at, 'company_name', v_company_name, 'inviter_name', v_inviter_name
  );
END;
$$;


ALTER FUNCTION "public"."create_invitation"("p_email" "text", "p_role" "public"."user_role") OWNER TO "postgres";


CREATE OR REPLACE FUNCTION "public"."create_trial_subscription_for_company"() RETURNS "trigger"
    LANGUAGE "plpgsql" SECURITY DEFINER
    SET "search_path" TO 'public'
    AS $$
BEGIN
  -- Create a trialing subscription record for the new company
  INSERT INTO company_subscriptions (company_id, status, trial_ends_at)
  VALUES (NEW.id, 'trialing', NOW() + INTERVAL '30 days')
  ON CONFLICT (company_id) DO NOTHING;

  RETURN NEW;
END;
$$;


ALTER FUNCTION "public"."create_trial_subscription_for_company"() OWNER TO "postgres";


CREATE OR REPLACE FUNCTION "public"."get_available_slots"("p_company_slug" "text", "p_date" "date") RETURNS TABLE("start_time" timestamp with time zone, "end_time" timestamp with time zone, "inspector_id" "uuid", "inspector_name" "text", "inspector_avatar_url" "text")
    LANGUAGE "plpgsql" SECURITY DEFINER
    SET "search_path" TO 'public'
    AS $$
DECLARE
  v_company_id UUID;
  v_day_of_week INT;
  v_duration INT;
  v_selection_mode inspector_selection_mode;
  v_timezone TEXT;
BEGIN
  SELECT id, default_inspection_duration_minutes, c.inspector_selection_mode, c.timezone
  INTO v_company_id, v_duration, v_selection_mode, v_timezone
  FROM companies c WHERE slug = p_company_slug;

  IF v_company_id IS NULL THEN
    RAISE EXCEPTION 'Company not found';
  END IF;

  IF v_timezone IS NULL THEN
    v_timezone := 'America/Los_Angeles';
  END IF;

  v_day_of_week := EXTRACT(DOW FROM p_date);

  IF v_selection_mode = 'admin_assigns' THEN
    RETURN QUERY
    WITH inspector_slots AS (
      SELECT
        a.user_id,
        ((p_date + a.start_time)::timestamp AT TIME ZONE v_timezone) AS window_start,
        ((p_date + a.end_time)::timestamp AT TIME ZONE v_timezone) AS window_end
      FROM availability_schedules a
      JOIN company_memberships cm ON cm.user_id = a.user_id AND cm.company_id = v_company_id
      WHERE a.company_id = v_company_id
        AND a.day_of_week = v_day_of_week
        AND a.is_active = true
        AND cm.role IN ('admin', 'inspector')
    ),
    existing_bookings AS (
      SELECT
        i.inspector_id AS booked_inspector_id,
        i.scheduled_date AS booking_start,
        i.scheduled_date + (COALESCE(i.duration_minutes, v_duration) * INTERVAL '1 minute') AS booking_end
      FROM inspections i
      WHERE i.company_id = v_company_id
        AND i.scheduled_date >= ((p_date)::timestamp AT TIME ZONE v_timezone)
        AND i.scheduled_date < ((p_date + INTERVAL '1 day')::timestamp AT TIME ZONE v_timezone)
        AND i.status IN ('scheduled', 'in_progress', 'requested')
        AND i.inspector_id IS NOT NULL
    ),
    time_slots AS (
      SELECT DISTINCT
        slot_time AS slot_start,
        slot_time + (v_duration * INTERVAL '1 minute') AS slot_end
      FROM inspector_slots s,
      LATERAL generate_series(
        s.window_start,
        s.window_end - (v_duration * INTERVAL '1 minute'),
        INTERVAL '30 minutes'
      ) AS slot_time
    ),
    available_slots AS (
      SELECT ts.slot_start, ts.slot_end
      FROM time_slots ts
      WHERE EXISTS (
        SELECT 1
        FROM inspector_slots ins
        WHERE ts.slot_start >= ins.window_start
          AND ts.slot_end <= ins.window_end
          AND NOT EXISTS (
            SELECT 1 FROM existing_bookings eb
            WHERE eb.booked_inspector_id = ins.user_id
              AND ts.slot_start < eb.booking_end
              AND ts.slot_end > eb.booking_start
          )
      )
    )
    SELECT
      avs.slot_start,
      avs.slot_end,
      NULL::uuid AS inspector_id,
      NULL::text AS inspector_name,
      NULL::text AS inspector_avatar_url
    FROM available_slots avs
    ORDER BY avs.slot_start;
  ELSE
    RETURN QUERY
    WITH inspector_slots AS (
      SELECT
        a.user_id,
        u.full_name,
        u.avatar_url,
        ((p_date + a.start_time)::timestamp AT TIME ZONE v_timezone) AS window_start,
        ((p_date + a.end_time)::timestamp AT TIME ZONE v_timezone) AS window_end
      FROM availability_schedules a
      JOIN users u ON u.id = a.user_id
      JOIN company_memberships cm ON cm.user_id = a.user_id AND cm.company_id = v_company_id
      WHERE a.company_id = v_company_id
        AND a.day_of_week = v_day_of_week
        AND a.is_active = true
        AND cm.role IN ('admin', 'inspector')
    ),
    existing_bookings AS (
      SELECT
        i.inspector_id AS booked_inspector_id,
        i.scheduled_date AS booking_start,
        i.scheduled_date + (COALESCE(i.duration_minutes, v_duration) * INTERVAL '1 minute') AS booking_end
      FROM inspections i
      WHERE i.company_id = v_company_id
        AND i.scheduled_date >= ((p_date)::timestamp AT TIME ZONE v_timezone)
        AND i.scheduled_date < ((p_date + INTERVAL '1 day')::timestamp AT TIME ZONE v_timezone)
        AND i.status IN ('scheduled', 'in_progress', 'requested')
    ),
    time_slots AS (
      SELECT
        s.user_id,
        s.full_name,
        s.avatar_url,
        slot_time AS slot_start,
        slot_time + (v_duration * INTERVAL '1 minute') AS slot_end
      FROM inspector_slots s,
      LATERAL generate_series(
        s.window_start,
        s.window_end - (v_duration * INTERVAL '1 minute'),
        INTERVAL '30 minutes'
      ) AS slot_time
    )
    SELECT
      ts.slot_start,
      ts.slot_end,
      ts.user_id,
      ts.full_name,
      ts.avatar_url
    FROM time_slots ts
    WHERE NOT EXISTS (
      SELECT 1 FROM existing_bookings eb
      WHERE eb.booked_inspector_id = ts.user_id
        AND ts.slot_start < eb.booking_end
        AND ts.slot_end > eb.booking_start
    )
    ORDER BY ts.slot_start, ts.full_name;
  END IF;
END;
$$;


ALTER FUNCTION "public"."get_available_slots"("p_company_slug" "text", "p_date" "date") OWNER TO "postgres";


CREATE OR REPLACE FUNCTION "public"."get_client_inspector_ids"() RETURNS SETOF "uuid"
    LANGUAGE "sql" STABLE SECURITY DEFINER
    SET "search_path" TO 'public'
    AS $$
  SELECT DISTINCT i.inspector_id
  FROM inspections i
  JOIN clients c ON c.id = i.client_id
  WHERE c.user_id = auth.uid()
  AND i.inspector_id IS NOT NULL
$$;


ALTER FUNCTION "public"."get_client_inspector_ids"() OWNER TO "postgres";


CREATE OR REPLACE FUNCTION "public"."get_client_invitation_details"("p_token" "text") RETURNS TABLE("id" "uuid", "client_id" "uuid", "email" "text", "company_id" "uuid", "company_name" "text", "client_name" "text", "expires_at" timestamp with time zone, "status" "text")
    LANGUAGE "plpgsql" SECURITY DEFINER
    SET "search_path" TO 'public'
    AS $$
BEGIN
  RETURN QUERY
  SELECT 
    ci.id,
    ci.client_id,
    ci.email,
    ci.company_id,
    c.name as company_name,
    cl.name as client_name,
    ci.expires_at,
    ci.status
  FROM client_invitations ci
  JOIN companies c ON c.id = ci.company_id
  JOIN clients cl ON cl.id = ci.client_id
  WHERE ci.token = p_token
    AND ci.status = 'pending';
END;
$$;


ALTER FUNCTION "public"."get_client_invitation_details"("p_token" "text") OWNER TO "postgres";


CREATE OR REPLACE FUNCTION "public"."get_company_by_slug"("p_slug" "text") RETURNS json
    LANGUAGE "sql" SECURITY DEFINER
    SET "search_path" TO 'public'
    AS $$
  SELECT json_build_object(
    'id', id,
    'name', name,
    'slug', slug,
    'logo_url', logo_url,
    'primary_color', primary_color,
    'booking_requires_confirmation', booking_requires_confirmation,
    'default_inspection_duration_minutes', default_inspection_duration_minutes,
    'inspector_selection_mode', inspector_selection_mode,
    'timezone', timezone
  )
  FROM companies
  WHERE slug = p_slug
$$;


ALTER FUNCTION "public"."get_company_by_slug"("p_slug" "text") OWNER TO "postgres";


CREATE OR REPLACE FUNCTION "public"."get_company_subscription_status"() RETURNS TABLE("status" "public"."subscription_status", "trial_ends_at" timestamp with time zone, "current_period_ends_at" timestamp with time zone, "billing_interval" "text")
    LANGUAGE "plpgsql" SECURITY DEFINER
    AS $$
BEGIN
  RETURN QUERY
  SELECT 
    cs.status,
    cs.trial_ends_at,
    cs.current_period_ends_at,
    cs.billing_interval
  FROM company_subscriptions cs
  WHERE cs.company_id = get_user_company_id();
END;
$$;


ALTER FUNCTION "public"."get_company_subscription_status"() OWNER TO "postgres";


CREATE OR REPLACE FUNCTION "public"."get_invitation_by_token"("p_token" "text") RETURNS json
    LANGUAGE "plpgsql" SECURITY DEFINER
    SET "search_path" TO 'public'
    AS $$
DECLARE
  v_invitation invitations;
  v_company_name text;
BEGIN
  SELECT * INTO v_invitation FROM invitations WHERE token = p_token;
  IF v_invitation IS NULL THEN
    RETURN json_build_object('error', 'Invalid invitation token');
  END IF;
  IF v_invitation.status != 'pending' THEN
    RETURN json_build_object('error', 'Invitation is no longer valid', 'status', v_invitation.status);
  END IF;
  IF v_invitation.expires_at < now() THEN
    UPDATE invitations SET status = 'expired' WHERE id = v_invitation.id;
    RETURN json_build_object('error', 'Invitation has expired');
  END IF;
  SELECT name INTO v_company_name FROM companies WHERE id = v_invitation.company_id;
  RETURN json_build_object(
    'id', v_invitation.id, 'email', v_invitation.email, 'role', v_invitation.role,
    'company_id', v_invitation.company_id, 'company_name', v_company_name, 'expires_at', v_invitation.expires_at
  );
END;
$$;


ALTER FUNCTION "public"."get_invitation_by_token"("p_token" "text") OWNER TO "postgres";


CREATE OR REPLACE FUNCTION "public"."get_platform_customer_clients"("p_company_id" "uuid") RETURNS TABLE("id" "uuid", "name" "text", "email" "text", "phone" "text", "client_type" "text", "status" "text", "has_portal_access" boolean, "inspection_count" bigint, "created_at" timestamp with time zone)
    LANGUAGE "plpgsql" SECURITY DEFINER
    SET "search_path" TO 'public'
    AS $$
BEGIN
  -- Verify caller has platform admin permission
  IF NOT has_platform_permission('admin') THEN
    RAISE EXCEPTION 'Access denied: requires platform admin permission';
  END IF;

  RETURN QUERY
  SELECT
    c.id,
    c.name,
    c.email,
    c.phone,
    c.client_type,
    c.status,
    (c.user_id IS NOT NULL) AS has_portal_access,
    (SELECT COUNT(*) FROM inspections i WHERE i.client_id = c.id) AS inspection_count,
    c.created_at
  FROM clients c
  WHERE c.company_id = p_company_id
  ORDER BY c.created_at DESC;
END;
$$;


ALTER FUNCTION "public"."get_platform_customer_clients"("p_company_id" "uuid") OWNER TO "postgres";


CREATE OR REPLACE FUNCTION "public"."get_platform_customer_detail"("p_company_id" "uuid") RETURNS TABLE("id" "uuid", "name" "text", "slug" "text", "created_at" timestamp with time zone, "stripe_customer_id" "text", "subscription_status" "public"."subscription_status", "billing_interval" "text", "trial_ends_at" timestamp with time zone, "current_period_ends_at" timestamp with time zone, "canceled_at" timestamp with time zone)
    LANGUAGE "plpgsql" SECURITY DEFINER
    SET "search_path" TO 'public'
    AS $$
BEGIN
  -- Verify caller has platform admin permission
  IF NOT has_platform_permission('admin') THEN
    RAISE EXCEPTION 'Access denied: requires platform admin permission';
  END IF;

  RETURN QUERY
  SELECT
    c.id,
    c.name,
    c.slug,
    c.created_at,
    cs.stripe_customer_id,
    cs.status AS subscription_status,
    cs.billing_interval,
    cs.trial_ends_at,
    cs.current_period_ends_at,
    cs.canceled_at
  FROM companies c
  LEFT JOIN company_subscriptions cs ON cs.company_id = c.id
  WHERE c.id = p_company_id;
END;
$$;


ALTER FUNCTION "public"."get_platform_customer_detail"("p_company_id" "uuid") OWNER TO "postgres";


CREATE OR REPLACE FUNCTION "public"."get_platform_customer_inspections"("p_company_id" "uuid") RETURNS TABLE("id" "uuid", "property_address" "text", "scheduled_date" timestamp with time zone, "status" "public"."inspection_status", "client_name" "text", "client_email" "text", "inspector_name" "text", "template_name" "text", "completion_percentage" integer, "created_at" timestamp with time zone)
    LANGUAGE "plpgsql" SECURITY DEFINER
    SET "search_path" TO 'public'
    AS $$
BEGIN
  -- Verify caller has platform admin permission
  IF NOT has_platform_permission('admin') THEN
    RAISE EXCEPTION 'Access denied: requires platform admin permission';
  END IF;

  RETURN QUERY
  SELECT
    i.id,
    i.property_address,
    i.scheduled_date,
    i.status,
    c.name AS client_name,
    c.email AS client_email,
    u.full_name AS inspector_name,
    it.name AS template_name,
    i.completion_percentage,
    i.created_at
  FROM inspections i
  LEFT JOIN clients c ON c.id = i.client_id
  LEFT JOIN users u ON u.id = i.inspector_id
  LEFT JOIN inspection_templates it ON it.id = i.template_id
  WHERE i.company_id = p_company_id
  ORDER BY i.scheduled_date DESC;
END;
$$;


ALTER FUNCTION "public"."get_platform_customer_inspections"("p_company_id" "uuid") OWNER TO "postgres";


CREATE OR REPLACE FUNCTION "public"."get_platform_customer_invitations"("p_company_id" "uuid") RETURNS TABLE("id" "uuid", "email" "text", "invitation_type" "text", "role" "text", "status" "text", "invited_by_name" "text", "expires_at" timestamp with time zone, "created_at" timestamp with time zone)
    LANGUAGE "plpgsql" SECURITY DEFINER
    SET "search_path" TO 'public'
    AS $$
BEGIN
  -- Verify caller has platform admin permission
  IF NOT has_platform_permission('admin') THEN
    RAISE EXCEPTION 'Access denied: requires platform admin permission';
  END IF;

  RETURN QUERY
  -- Team invitations
  SELECT
    i.id,
    i.email,
    'team'::TEXT AS invitation_type,
    i.role::TEXT,
    i.status::TEXT,
    u.full_name AS invited_by_name,
    i.expires_at,
    i.created_at
  FROM invitations i
  LEFT JOIN users u ON u.id = i.invited_by
  WHERE i.company_id = p_company_id
  
  UNION ALL
  
  -- Client invitations
  SELECT
    ci.id,
    ci.email,
    'client'::TEXT AS invitation_type,
    'client'::TEXT AS role,
    ci.status::TEXT,
    u.full_name AS invited_by_name,
    ci.expires_at,
    ci.created_at
  FROM client_invitations ci
  LEFT JOIN users u ON u.id = ci.invited_by
  WHERE ci.company_id = p_company_id
  
  ORDER BY created_at DESC;
END;
$$;


ALTER FUNCTION "public"."get_platform_customer_invitations"("p_company_id" "uuid") OWNER TO "postgres";


CREATE OR REPLACE FUNCTION "public"."get_platform_customer_invoices"("p_company_id" "uuid") RETURNS TABLE("id" "uuid", "stripe_invoice_id" "text", "amount_due" integer, "amount_paid" integer, "status" "public"."invoice_status", "invoice_url" "text", "period_start" timestamp with time zone, "period_end" timestamp with time zone, "created_at" timestamp with time zone, "total_paid" bigint, "invoice_count" bigint)
    LANGUAGE "plpgsql" SECURITY DEFINER
    SET "search_path" TO 'public'
    AS $$
DECLARE
  v_total_paid BIGINT;
  v_invoice_count BIGINT;
BEGIN
  -- Verify caller has platform admin permission
  IF NOT has_platform_permission('admin') THEN
    RAISE EXCEPTION 'Access denied: requires platform admin permission';
  END IF;

  -- Calculate totals
  SELECT
    COALESCE(SUM(si.amount_paid), 0),
    COUNT(*)
  INTO v_total_paid, v_invoice_count
  FROM subscription_invoices si
  WHERE si.company_id = p_company_id;

  -- Return invoices with totals
  RETURN QUERY
  SELECT
    si.id,
    si.stripe_invoice_id,
    si.amount_due,
    si.amount_paid,
    si.status,
    si.invoice_url,
    si.period_start,
    si.period_end,
    si.created_at,
    v_total_paid AS total_paid,
    v_invoice_count AS invoice_count
  FROM subscription_invoices si
  WHERE si.company_id = p_company_id
  ORDER BY si.created_at DESC;
END;
$$;


ALTER FUNCTION "public"."get_platform_customer_invoices"("p_company_id" "uuid") OWNER TO "postgres";


CREATE OR REPLACE FUNCTION "public"."get_platform_customer_members"("p_company_id" "uuid") RETURNS TABLE("id" "uuid", "full_name" "text", "email" "text", "role" "public"."user_role", "created_at" timestamp with time zone)
    LANGUAGE "plpgsql" SECURITY DEFINER
    SET "search_path" TO 'public'
    AS $$
BEGIN
  -- Verify caller has platform admin permission
  IF NOT has_platform_permission('admin') THEN
    RAISE EXCEPTION 'Access denied: requires platform admin permission';
  END IF;

  RETURN QUERY
  SELECT
    u.id,
    u.full_name,
    u.email,
    cm.role,
    cm.created_at
  FROM users u
  INNER JOIN company_memberships cm ON cm.user_id = u.id
  WHERE cm.company_id = p_company_id
  ORDER BY cm.role, u.full_name;
END;
$$;


ALTER FUNCTION "public"."get_platform_customer_members"("p_company_id" "uuid") OWNER TO "postgres";


CREATE OR REPLACE FUNCTION "public"."get_platform_customer_templates"("p_company_id" "uuid") RETURNS TABLE("id" "uuid", "name" "text", "description" "text", "category" "text", "is_active" boolean, "base_price" numeric, "created_at" timestamp with time zone, "updated_at" timestamp with time zone)
    LANGUAGE "plpgsql" SECURITY DEFINER
    SET "search_path" TO 'public'
    AS $$
BEGIN
  -- Verify caller has platform admin permission
  IF NOT has_platform_permission('admin') THEN
    RAISE EXCEPTION 'Access denied: requires platform admin permission';
  END IF;

  RETURN QUERY
  SELECT
    it.id,
    it.name,
    it.description,
    it.category,
    it.is_active,
    it.base_price,
    it.created_at,
    it.updated_at
  FROM inspection_templates it
  WHERE it.company_id = p_company_id
  ORDER BY it.is_active DESC, it.name;
END;
$$;


ALTER FUNCTION "public"."get_platform_customer_templates"("p_company_id" "uuid") OWNER TO "postgres";


CREATE OR REPLACE FUNCTION "public"."get_platform_customers"("p_search_term" "text" DEFAULT NULL::"text", "p_page" integer DEFAULT 1, "p_page_size" integer DEFAULT 25) RETURNS TABLE("id" "uuid", "name" "text", "slug" "text", "created_at" timestamp with time zone, "subscription_status" "public"."subscription_status", "billing_interval" "text", "trial_ends_at" timestamp with time zone, "current_period_ends_at" timestamp with time zone, "total_count" bigint)
    LANGUAGE "plpgsql" SECURITY DEFINER
    SET "search_path" TO 'public'
    AS $$
DECLARE
  v_offset INT;
  v_total BIGINT;
BEGIN
  -- Verify caller has platform admin permission
  IF NOT has_platform_permission('admin') THEN
    RAISE EXCEPTION 'Access denied: requires platform admin permission';
  END IF;

  v_offset := (p_page - 1) * p_page_size;

  -- Get total count for pagination
  SELECT COUNT(DISTINCT c.id) INTO v_total
  FROM companies c
  LEFT JOIN users u ON u.company_id = c.id
  LEFT JOIN company_memberships cm ON cm.company_id = c.id
  LEFT JOIN users mu ON mu.id = cm.user_id
  WHERE p_search_term IS NULL
    OR LENGTH(p_search_term) < 2
    OR c.name ILIKE '%' || p_search_term || '%'
    OR u.email ILIKE '%' || p_search_term || '%'
    OR mu.email ILIKE '%' || p_search_term || '%';

  -- Return paginated results
  RETURN QUERY
  SELECT DISTINCT ON (c.id)
    c.id,
    c.name,
    c.slug,
    c.created_at,
    cs.status AS subscription_status,
    cs.billing_interval,
    cs.trial_ends_at,
    cs.current_period_ends_at,
    v_total AS total_count
  FROM companies c
  LEFT JOIN company_subscriptions cs ON cs.company_id = c.id
  LEFT JOIN users u ON u.company_id = c.id
  LEFT JOIN company_memberships cm ON cm.company_id = c.id
  LEFT JOIN users mu ON mu.id = cm.user_id
  WHERE p_search_term IS NULL
    OR LENGTH(p_search_term) < 2
    OR c.name ILIKE '%' || p_search_term || '%'
    OR u.email ILIKE '%' || p_search_term || '%'
    OR mu.email ILIKE '%' || p_search_term || '%'
  ORDER BY c.id, c.name
  LIMIT p_page_size
  OFFSET v_offset;
END;
$$;


ALTER FUNCTION "public"."get_platform_customers"("p_search_term" "text", "p_page" integer, "p_page_size" integer) OWNER TO "postgres";


CREATE OR REPLACE FUNCTION "public"."get_platform_staff"() RETURNS TABLE("id" "uuid", "user_id" "uuid", "created_at" timestamp with time zone, "email" "text", "full_name" "text", "avatar_url" "text", "permissions" "public"."platform_permission"[])
    LANGUAGE "plpgsql" SECURITY DEFINER
    SET "search_path" TO 'public'
    AS $$
BEGIN
  -- Only allow platform staff with manage_platform_staff permission
  IF NOT has_platform_permission('manage_platform_staff') THEN
    RAISE EXCEPTION 'Permission denied';
  END IF;

  RETURN QUERY
  SELECT 
    ps.id,
    ps.user_id,
    ps.created_at,
    u.email,
    u.full_name,
    u.avatar_url,
    COALESCE(array_agg(psp.permission) FILTER (WHERE psp.permission IS NOT NULL), '{}')::platform_permission[] as permissions
  FROM platform_staff ps
  JOIN users u ON u.id = ps.user_id
  LEFT JOIN platform_staff_permissions psp ON psp.user_id = ps.user_id
  GROUP BY ps.id, ps.user_id, ps.created_at, u.email, u.full_name, u.avatar_url
  ORDER BY ps.created_at;
END;
$$;


ALTER FUNCTION "public"."get_platform_staff"() OWNER TO "postgres";


CREATE OR REPLACE FUNCTION "public"."get_user_companies"() RETURNS json
    LANGUAGE "plpgsql" SECURITY DEFINER
    SET "search_path" TO 'public'
    AS $$
BEGIN
  RETURN (
    SELECT json_agg(json_build_object(
      'id', c.id,
      'name', c.name,
      'slug', c.slug,
      'role', cm.role,
      'is_current', c.id = u.current_company_id,
      'logo_url', c.logo_url
    ))
    FROM company_memberships cm
    JOIN companies c ON c.id = cm.company_id
    JOIN users u ON u.id = cm.user_id
    WHERE cm.user_id = auth.uid()
  );
END;
$$;


ALTER FUNCTION "public"."get_user_companies"() OWNER TO "postgres";


CREATE OR REPLACE FUNCTION "public"."get_user_company_id"() RETURNS "uuid"
    LANGUAGE "sql" STABLE SECURITY DEFINER
    SET "search_path" TO 'public'
    AS $$
  SELECT current_company_id FROM public.users WHERE id = auth.uid()
$$;


ALTER FUNCTION "public"."get_user_company_id"() OWNER TO "postgres";


CREATE OR REPLACE FUNCTION "public"."get_user_role"() RETURNS "public"."user_role"
    LANGUAGE "sql" STABLE SECURITY DEFINER
    SET "search_path" TO 'public'
    AS $$
  SELECT role FROM public.company_memberships
  WHERE user_id = auth.uid()
    AND company_id = get_user_company_id()
$$;


ALTER FUNCTION "public"."get_user_role"() OWNER TO "postgres";


CREATE OR REPLACE FUNCTION "public"."has_platform_permission"("required_permission" "public"."platform_permission") RETURNS boolean
    LANGUAGE "sql" STABLE SECURITY DEFINER
    SET "search_path" TO 'public'
    AS $$
  SELECT EXISTS (
    SELECT 1 FROM public.platform_staff_permissions
    WHERE user_id = auth.uid()
    AND (
      permission = 'admin'  -- Admin has all permissions
      OR permission = required_permission
      OR (required_permission = 'billing_support' AND permission = 'billing_admin')
    )
  )
$$;


ALTER FUNCTION "public"."has_platform_permission"("required_permission" "public"."platform_permission") OWNER TO "postgres";


CREATE OR REPLACE FUNCTION "public"."insert_default_availability"("p_user_id" "uuid", "p_company_id" "uuid") RETURNS "void"
    LANGUAGE "plpgsql" SECURITY DEFINER
    SET "search_path" TO 'public'
    AS $$
BEGIN
  -- Insert default availability for all 7 days
  -- Monday-Friday: 8am-5pm, active
  -- Saturday-Sunday: 8am-5pm, inactive (user can enable if needed)
  INSERT INTO availability_schedules (user_id, company_id, day_of_week, start_time, end_time, is_active)
  VALUES
    (p_user_id, p_company_id, 0, '08:00'::time, '17:00'::time, false),  -- Sunday
    (p_user_id, p_company_id, 1, '08:00'::time, '17:00'::time, true),   -- Monday
    (p_user_id, p_company_id, 2, '08:00'::time, '17:00'::time, true),   -- Tuesday
    (p_user_id, p_company_id, 3, '08:00'::time, '17:00'::time, true),   -- Wednesday
    (p_user_id, p_company_id, 4, '08:00'::time, '17:00'::time, true),   -- Thursday
    (p_user_id, p_company_id, 5, '08:00'::time, '17:00'::time, true),   -- Friday
    (p_user_id, p_company_id, 6, '08:00'::time, '17:00'::time, false)   -- Saturday
  ON CONFLICT (user_id, day_of_week) DO NOTHING;  -- Don't overwrite if already exists
END;
$$;


ALTER FUNCTION "public"."insert_default_availability"("p_user_id" "uuid", "p_company_id" "uuid") OWNER TO "postgres";


CREATE OR REPLACE FUNCTION "public"."is_admin"() RETURNS boolean
    LANGUAGE "sql" STABLE SECURITY DEFINER
    SET "search_path" TO 'public'
    AS $$
  SELECT get_user_role() = 'admin'
$$;


ALTER FUNCTION "public"."is_admin"() OWNER TO "postgres";


CREATE OR REPLACE FUNCTION "public"."is_platform_staff"() RETURNS boolean
    LANGUAGE "sql" STABLE SECURITY DEFINER
    SET "search_path" TO 'public'
    AS $$
  SELECT EXISTS (SELECT 1 FROM public.platform_staff WHERE user_id = auth.uid())
$$;


ALTER FUNCTION "public"."is_platform_staff"() OWNER TO "postgres";


CREATE OR REPLACE FUNCTION "public"."is_staff"() RETURNS boolean
    LANGUAGE "sql" STABLE SECURITY DEFINER
    SET "search_path" TO 'public'
    AS $$
  SELECT COALESCE(get_user_role() IN ('admin', 'inspector'), false)
$$;


ALTER FUNCTION "public"."is_staff"() OWNER TO "postgres";


CREATE OR REPLACE FUNCTION "public"."mark_expired_invitations"() RETURNS "void"
    LANGUAGE "plpgsql" SECURITY DEFINER
    SET "search_path" TO 'public'
    AS $$
DECLARE
  team_count int;
  client_count int;
BEGIN
  -- Mark expired team invitations
  UPDATE invitations
  SET status = 'expired'
  WHERE status = 'pending'
  AND expires_at < now();

  GET DIAGNOSTICS team_count = ROW_COUNT;

  -- Mark expired client invitations
  UPDATE client_invitations
  SET status = 'expired', updated_at = now()
  WHERE status = 'pending'
  AND expires_at < now();

  GET DIAGNOSTICS client_count = ROW_COUNT;

  -- Log if any were updated (optional, helps with debugging)
  IF team_count > 0 OR client_count > 0 THEN
    RAISE LOG 'mark_expired_invitations: updated % team invitations, % client invitations', team_count, client_count;
  END IF;
END;
$$;


ALTER FUNCTION "public"."mark_expired_invitations"() OWNER TO "postgres";


CREATE OR REPLACE FUNCTION "public"."resend_invitation"("p_invitation_id" "uuid") RETURNS json
    LANGUAGE "plpgsql" SECURITY DEFINER
    SET "search_path" TO 'public'
    AS $$
DECLARE
  v_invitation invitations;
  v_company_name text;
  v_inviter_name text;
BEGIN
  IF NOT is_admin() THEN
    RAISE EXCEPTION 'Only admins can resend invitations';
  END IF;
  UPDATE invitations SET expires_at = now() + interval '7 days'
  WHERE id = p_invitation_id AND company_id = get_user_company_id() AND status = 'pending'
  RETURNING * INTO v_invitation;
  IF v_invitation IS NULL THEN
    RAISE EXCEPTION 'Invitation not found or not pending';
  END IF;
  SELECT name INTO v_company_name FROM companies WHERE id = v_invitation.company_id;
  SELECT full_name INTO v_inviter_name FROM users WHERE id = auth.uid();
  RETURN json_build_object(
    'id', v_invitation.id, 'token', v_invitation.token, 'email', v_invitation.email, 'role', v_invitation.role,
    'expires_at', v_invitation.expires_at, 'company_name', v_company_name, 'inviter_name', v_inviter_name
  );
END;
$$;


ALTER FUNCTION "public"."resend_invitation"("p_invitation_id" "uuid") OWNER TO "postgres";


CREATE OR REPLACE FUNCTION "public"."revoke_invitation"("p_invitation_id" "uuid") RETURNS boolean
    LANGUAGE "plpgsql" SECURITY DEFINER
    SET "search_path" TO 'public'
    AS $$
BEGIN
  IF NOT is_admin() THEN
    RAISE EXCEPTION 'Only admins can revoke invitations';
  END IF;
  UPDATE invitations SET status = 'revoked'
  WHERE id = p_invitation_id AND company_id = get_user_company_id() AND status = 'pending';
  RETURN FOUND;
END;
$$;


ALTER FUNCTION "public"."revoke_invitation"("p_invitation_id" "uuid") OWNER TO "postgres";


CREATE OR REPLACE FUNCTION "public"."switch_company"("p_company_id" "uuid") RETURNS json
    LANGUAGE "plpgsql" SECURITY DEFINER
    SET "search_path" TO 'public'
    AS $$
DECLARE
  v_membership company_memberships;
  v_company companies;
BEGIN
  SELECT * INTO v_membership FROM company_memberships WHERE user_id = auth.uid() AND company_id = p_company_id;
  IF v_membership IS NULL THEN
    RAISE EXCEPTION 'Not a member of this company';
  END IF;
  UPDATE users SET current_company_id = p_company_id WHERE id = auth.uid();
  SELECT * INTO v_company FROM companies WHERE id = p_company_id;
  RETURN json_build_object('company_id', p_company_id, 'company_name', v_company.name, 'role', v_membership.role);
END;
$$;


ALTER FUNCTION "public"."switch_company"("p_company_id" "uuid") OWNER TO "postgres";


CREATE OR REPLACE FUNCTION "public"."temp_read_storage_file"("bucket" "text", "file_path" "text") RETURNS "text"
    LANGUAGE "plpgsql" SECURITY DEFINER
    AS $$
DECLARE
  result text;
BEGIN
  SELECT convert_from(
    extensions.http_get(
      format('http://localhost:54321/storage/v1/object/%s/%s', bucket, file_path)
    )::bytea,
    'UTF8'
  ) INTO result;
  RETURN result;
EXCEPTION WHEN OTHERS THEN
  RETURN NULL;
END;
$$;


ALTER FUNCTION "public"."temp_read_storage_file"("bucket" "text", "file_path" "text") OWNER TO "postgres";


CREATE OR REPLACE FUNCTION "public"."update_feedback_updated_at"() RETURNS "trigger"
    LANGUAGE "plpgsql"
    AS $$
BEGIN
  NEW.updated_at = now();
  RETURN NEW;
END;
$$;


ALTER FUNCTION "public"."update_feedback_updated_at"() OWNER TO "postgres";


CREATE OR REPLACE FUNCTION "public"."update_sample_template_counts"() RETURNS "trigger"
    LANGUAGE "plpgsql"
    AS $$
DECLARE
  counts record;
BEGIN
  SELECT * INTO counts FROM count_template_structure(NEW.structure);
  NEW.section_count := counts.sections;
  NEW.field_count := counts.fields;
  RETURN NEW;
END;
$$;


ALTER FUNCTION "public"."update_sample_template_counts"() OWNER TO "postgres";


CREATE OR REPLACE FUNCTION "public"."update_updated_at"() RETURNS "trigger"
    LANGUAGE "plpgsql" SECURITY DEFINER
    SET "search_path" TO 'public'
    AS $$
BEGIN
  NEW.updated_at = now();
  RETURN NEW;
END;
$$;


ALTER FUNCTION "public"."update_updated_at"() OWNER TO "postgres";

SET default_tablespace = '';

SET default_table_access_method = "heap";


CREATE TABLE IF NOT EXISTS "public"."availability_schedules" (
    "id" "uuid" DEFAULT "gen_random_uuid"() NOT NULL,
    "company_id" "uuid" NOT NULL,
    "user_id" "uuid" NOT NULL,
    "day_of_week" integer NOT NULL,
    "start_time" time without time zone NOT NULL,
    "end_time" time without time zone NOT NULL,
    "is_active" boolean DEFAULT true,
    "created_at" timestamp with time zone DEFAULT "now"(),
    "updated_at" timestamp with time zone DEFAULT "now"(),
    CONSTRAINT "availability_schedules_day_of_week_check" CHECK ((("day_of_week" >= 0) AND ("day_of_week" <= 6)))
);


ALTER TABLE "public"."availability_schedules" OWNER TO "postgres";


CREATE TABLE IF NOT EXISTS "public"."client_invitations" (
    "id" "uuid" DEFAULT "extensions"."uuid_generate_v4"() NOT NULL,
    "company_id" "uuid" NOT NULL,
    "client_id" "uuid" NOT NULL,
    "email" "text" NOT NULL,
    "token" "text" NOT NULL,
    "status" "text" DEFAULT 'pending'::"text",
    "invited_by" "uuid",
    "expires_at" timestamp with time zone NOT NULL,
    "created_at" timestamp with time zone DEFAULT "now"(),
    "updated_at" timestamp with time zone DEFAULT "now"(),
    CONSTRAINT "client_invitations_status_check" CHECK (("status" = ANY (ARRAY['pending'::"text", 'accepted'::"text", 'expired'::"text", 'revoked'::"text"])))
);


ALTER TABLE "public"."client_invitations" OWNER TO "postgres";


CREATE TABLE IF NOT EXISTS "public"."clients" (
    "id" "uuid" DEFAULT "extensions"."uuid_generate_v4"() NOT NULL,
    "company_id" "uuid" NOT NULL,
    "name" "text" NOT NULL,
    "email" "text",
    "phone" "text",
    "address" "text",
    "created_at" timestamp with time zone DEFAULT "now"() NOT NULL,
    "updated_at" timestamp with time zone DEFAULT "now"() NOT NULL,
    "client_type" "text" DEFAULT 'homeowner'::"text",
    "user_id" "uuid",
    "status" "text" DEFAULT 'prospect'::"text",
    "notes" "text",
    CONSTRAINT "clients_client_type_check" CHECK (("client_type" = ANY (ARRAY['homeowner'::"text", 'agent'::"text"]))),
    CONSTRAINT "clients_status_check" CHECK (("status" = ANY (ARRAY['prospect'::"text", 'invited'::"text", 'active'::"text", 'inactive'::"text"])))
);


ALTER TABLE "public"."clients" OWNER TO "postgres";


CREATE TABLE IF NOT EXISTS "public"."companies" (
    "id" "uuid" DEFAULT "extensions"."uuid_generate_v4"() NOT NULL,
    "name" "text" NOT NULL,
    "created_at" timestamp with time zone DEFAULT "now"() NOT NULL,
    "updated_at" timestamp with time zone DEFAULT "now"() NOT NULL,
    "slug" "text" NOT NULL,
    "logo_url" "text",
    "primary_color" "text" DEFAULT '#3b82f6'::"text",
    "booking_requires_confirmation" boolean DEFAULT true,
    "default_inspection_duration_minutes" integer DEFAULT 120,
    "flags" "jsonb",
    "inspector_selection_mode" "public"."inspector_selection_mode" DEFAULT 'client_chooses'::"public"."inspector_selection_mode",
    "timezone" "text" DEFAULT 'America/Los_Angeles'::"text"
);


ALTER TABLE "public"."companies" OWNER TO "postgres";


COMMENT ON COLUMN "public"."companies"."flags" IS 'Company-wide flag definitions for inspection items. Array of {id, name, color, icon} objects.';



COMMENT ON COLUMN "public"."companies"."inspector_selection_mode" IS 'Controls how inspectors are assigned during booking: client_chooses (default) shows inspector names/photos, admin_assigns shows anonymous time slots';



CREATE TABLE IF NOT EXISTS "public"."company_memberships" (
    "id" "uuid" DEFAULT "gen_random_uuid"() NOT NULL,
    "user_id" "uuid" NOT NULL,
    "company_id" "uuid" NOT NULL,
    "role" "public"."user_role" DEFAULT 'inspector'::"public"."user_role" NOT NULL,
    "created_at" timestamp with time zone DEFAULT "now"() NOT NULL
);


ALTER TABLE "public"."company_memberships" OWNER TO "postgres";


CREATE TABLE IF NOT EXISTS "public"."company_subscriptions" (
    "id" "uuid" DEFAULT "gen_random_uuid"() NOT NULL,
    "company_id" "uuid" NOT NULL,
    "stripe_customer_id" "text",
    "stripe_subscription_id" "text",
    "status" "public"."subscription_status" DEFAULT 'trialing'::"public"."subscription_status" NOT NULL,
    "billing_interval" "text",
    "trial_ends_at" timestamp with time zone,
    "current_period_ends_at" timestamp with time zone,
    "canceled_at" timestamp with time zone,
    "created_at" timestamp with time zone DEFAULT "now"() NOT NULL,
    "updated_at" timestamp with time zone DEFAULT "now"() NOT NULL,
    "trial_reminder_emails_sent" "text"[] DEFAULT '{}'::"text"[],
    CONSTRAINT "company_subscriptions_billing_interval_check" CHECK (("billing_interval" = ANY (ARRAY['month'::"text", 'year'::"text"])))
);


ALTER TABLE "public"."company_subscriptions" OWNER TO "postgres";


COMMENT ON COLUMN "public"."company_subscriptions"."trial_reminder_emails_sent" IS 'Tracks which trial reminder emails have been sent (7_day, 1_day) to prevent duplicates';



CREATE TABLE IF NOT EXISTS "public"."feedback" (
    "id" "uuid" DEFAULT "gen_random_uuid"() NOT NULL,
    "user_id" "uuid" NOT NULL,
    "company_id" "uuid",
    "category" "public"."feedback_category" NOT NULL,
    "subject" "text" NOT NULL,
    "message" "text" NOT NULL,
    "page_url" "text",
    "user_agent" "text",
    "viewport_size" "text",
    "metadata" "jsonb" DEFAULT '{}'::"jsonb",
    "status" "public"."feedback_status" DEFAULT 'new'::"public"."feedback_status",
    "created_at" timestamp with time zone DEFAULT "now"(),
    "updated_at" timestamp with time zone DEFAULT "now"()
);


ALTER TABLE "public"."feedback" OWNER TO "postgres";


COMMENT ON TABLE "public"."feedback" IS 'User feedback submissions from the app';



CREATE TABLE IF NOT EXISTS "public"."feedback_attachments" (
    "id" "uuid" DEFAULT "gen_random_uuid"() NOT NULL,
    "feedback_id" "uuid" NOT NULL,
    "storage_path" "text" NOT NULL,
    "filename" "text" NOT NULL,
    "mime_type" "text" NOT NULL,
    "size_bytes" integer NOT NULL,
    "created_at" timestamp with time zone DEFAULT "now"()
);


ALTER TABLE "public"."feedback_attachments" OWNER TO "postgres";


COMMENT ON TABLE "public"."feedback_attachments" IS 'File attachments for feedback submissions';



CREATE TABLE IF NOT EXISTS "public"."google_calendar_connections" (
    "id" "uuid" DEFAULT "gen_random_uuid"() NOT NULL,
    "user_id" "uuid" NOT NULL,
    "access_token" "text" NOT NULL,
    "refresh_token" "text" NOT NULL,
    "token_expires_at" timestamp with time zone NOT NULL,
    "calendar_id" "text",
    "created_at" timestamp with time zone DEFAULT "now"(),
    "updated_at" timestamp with time zone DEFAULT "now"()
);


ALTER TABLE "public"."google_calendar_connections" OWNER TO "postgres";


CREATE TABLE IF NOT EXISTS "public"."inspection_email_log" (
    "id" "uuid" DEFAULT "gen_random_uuid"() NOT NULL,
    "inspection_id" "uuid" NOT NULL,
    "email_type" "text" NOT NULL,
    "recipient_email" "text" NOT NULL,
    "sent_at" timestamp with time zone DEFAULT "now"(),
    "status" "text" DEFAULT 'sent'::"text",
    "error_message" "text",
    CONSTRAINT "inspection_email_log_email_type_check" CHECK (("email_type" = ANY (ARRAY['confirmation'::"text", 'update'::"text"])))
);


ALTER TABLE "public"."inspection_email_log" OWNER TO "postgres";


CREATE TABLE IF NOT EXISTS "public"."inspection_templates" (
    "id" "uuid" DEFAULT "gen_random_uuid"() NOT NULL,
    "company_id" "uuid" NOT NULL,
    "name" "text" NOT NULL,
    "description" "text",
    "is_active" boolean DEFAULT true NOT NULL,
    "base_price" numeric(10,2),
    "created_at" timestamp with time zone DEFAULT "now"() NOT NULL,
    "updated_at" timestamp with time zone DEFAULT "now"() NOT NULL,
    "structure_storage_path" "text",
    "category" "text"
);


ALTER TABLE "public"."inspection_templates" OWNER TO "postgres";


COMMENT ON COLUMN "public"."inspection_templates"."structure_storage_path" IS 'Path to template JSON in storage: {company_id}/{template_id}.json (primary storage for template structure)';



CREATE TABLE IF NOT EXISTS "public"."inspections" (
    "id" "uuid" DEFAULT "extensions"."uuid_generate_v4"() NOT NULL,
    "company_id" "uuid" NOT NULL,
    "client_id" "uuid" NOT NULL,
    "inspector_id" "uuid",
    "property_address" "text" NOT NULL,
    "scheduled_date" timestamp with time zone NOT NULL,
    "status" "public"."inspection_status" DEFAULT 'scheduled'::"public"."inspection_status" NOT NULL,
    "notes" "text",
    "created_at" timestamp with time zone DEFAULT "now"() NOT NULL,
    "updated_at" timestamp with time zone DEFAULT "now"() NOT NULL,
    "duration_minutes" integer DEFAULT 120,
    "google_event_id" "text",
    "template_id" "uuid",
    "data" "jsonb",
    "data_template_id" "uuid",
    "data_storage_path" "text",
    "sections_completed" integer DEFAULT 0,
    "sections_total" integer DEFAULT 0,
    "flagged_items_count" integer DEFAULT 0,
    "completion_percentage" integer DEFAULT 0,
    "last_synced_at" timestamp with time zone,
    "generated_reports" "jsonb" DEFAULT '{}'::"jsonb",
    "report_storage_path" "text",
    "report_generated_at" timestamp with time zone
);


ALTER TABLE "public"."inspections" OWNER TO "postgres";


COMMENT ON COLUMN "public"."inspections"."data" IS 'DEPRECATED: Use data_storage_path instead. Will be removed after migration.';



COMMENT ON COLUMN "public"."inspections"."data_storage_path" IS 'Path to inspection data JSON in storage: {company_id}/{inspection_id}/data.json';



COMMENT ON COLUMN "public"."inspections"."generated_reports" IS 'Generated PDF reports metadata keyed by locale. Structure: { locale: { generated_at, storage_path } }';



COMMENT ON COLUMN "public"."inspections"."report_storage_path" IS 'Path to generated PDF report in reports storage bucket';



COMMENT ON COLUMN "public"."inspections"."report_generated_at" IS 'Timestamp when report was last generated';



CREATE TABLE IF NOT EXISTS "public"."invitations" (
    "id" "uuid" DEFAULT "gen_random_uuid"() NOT NULL,
    "company_id" "uuid" NOT NULL,
    "email" "text" NOT NULL,
    "role" "public"."user_role" DEFAULT 'inspector'::"public"."user_role" NOT NULL,
    "token" "text" NOT NULL,
    "invited_by" "uuid",
    "status" "public"."invite_status" DEFAULT 'pending'::"public"."invite_status" NOT NULL,
    "created_at" timestamp with time zone DEFAULT "now"() NOT NULL,
    "expires_at" timestamp with time zone DEFAULT ("now"() + '7 days'::interval) NOT NULL,
    "accepted_at" timestamp with time zone
);


ALTER TABLE "public"."invitations" OWNER TO "postgres";


CREATE TABLE IF NOT EXISTS "public"."llm_usage_logs" (
    "id" "uuid" DEFAULT "gen_random_uuid"() NOT NULL,
    "created_at" timestamp with time zone DEFAULT "now"() NOT NULL,
    "company_id" "uuid",
    "user_id" "uuid",
    "function_name" "text" NOT NULL,
    "model" "text" NOT NULL,
    "metadata" "jsonb" DEFAULT '{}'::"jsonb",
    "input_tokens" integer,
    "output_tokens" integer,
    "total_tokens" integer,
    "cost_usd" numeric(10,6),
    "success" boolean DEFAULT true NOT NULL,
    "error_message" "text",
    "duration_ms" integer,
    CONSTRAINT "llm_usage_logs_tokens_check" CHECK (((("input_tokens" IS NULL) AND ("output_tokens" IS NULL) AND ("total_tokens" IS NULL)) OR (("input_tokens" >= 0) AND ("output_tokens" >= 0))))
);


ALTER TABLE "public"."llm_usage_logs" OWNER TO "postgres";


COMMENT ON TABLE "public"."llm_usage_logs" IS 'Logs all LLM API calls for cost tracking and analytics';



CREATE TABLE IF NOT EXISTS "public"."platform_coupons" (
    "id" "uuid" DEFAULT "gen_random_uuid"() NOT NULL,
    "stripe_coupon_id" "text" NOT NULL,
    "name" "text" NOT NULL,
    "discount_type" "text" NOT NULL,
    "percent_off" numeric,
    "amount_off" integer,
    "currency" "text",
    "duration" "text" NOT NULL,
    "duration_in_months" integer,
    "max_redemptions" integer,
    "times_redeemed" integer DEFAULT 0 NOT NULL,
    "is_active" boolean DEFAULT true NOT NULL,
    "created_at" timestamp with time zone DEFAULT "now"() NOT NULL,
    "expires_at" timestamp with time zone,
    CONSTRAINT "platform_coupons_amount_off_check" CHECK ((("amount_off" IS NULL) OR ("amount_off" > 0))),
    CONSTRAINT "platform_coupons_discount_type_check" CHECK (("discount_type" = ANY (ARRAY['percent_off'::"text", 'amount_off'::"text"]))),
    CONSTRAINT "platform_coupons_duration_check" CHECK (("duration" = ANY (ARRAY['once'::"text", 'repeating'::"text", 'forever'::"text"]))),
    CONSTRAINT "platform_coupons_duration_in_months_check" CHECK ((("duration_in_months" IS NULL) OR ("duration_in_months" > 0))),
    CONSTRAINT "platform_coupons_max_redemptions_check" CHECK ((("max_redemptions" IS NULL) OR ("max_redemptions" > 0))),
    CONSTRAINT "platform_coupons_percent_off_check" CHECK ((("percent_off" IS NULL) OR (("percent_off" > (0)::numeric) AND ("percent_off" <= (100)::numeric)))),
    CONSTRAINT "valid_duration_months" CHECK (((("duration" = 'repeating'::"text") AND ("duration_in_months" IS NOT NULL)) OR (("duration" <> 'repeating'::"text") AND ("duration_in_months" IS NULL)))),
    CONSTRAINT "valid_percent_off" CHECK (((("discount_type" = 'percent_off'::"text") AND ("percent_off" IS NOT NULL) AND ("amount_off" IS NULL)) OR (("discount_type" = 'amount_off'::"text") AND ("amount_off" IS NOT NULL) AND ("percent_off" IS NULL) AND ("currency" IS NOT NULL))))
);


ALTER TABLE "public"."platform_coupons" OWNER TO "postgres";


COMMENT ON TABLE "public"."platform_coupons" IS 'Synced copy of Stripe coupons created via platform admin. Stripe is source of truth.';



CREATE TABLE IF NOT EXISTS "public"."platform_promotion_codes" (
    "id" "uuid" DEFAULT "gen_random_uuid"() NOT NULL,
    "coupon_id" "uuid" NOT NULL,
    "stripe_promotion_code_id" "text" NOT NULL,
    "code" "text" NOT NULL,
    "max_redemptions" integer,
    "times_redeemed" integer DEFAULT 0 NOT NULL,
    "is_active" boolean DEFAULT true NOT NULL,
    "for_customer_id" "uuid",
    "created_at" timestamp with time zone DEFAULT "now"() NOT NULL,
    "expires_at" timestamp with time zone,
    CONSTRAINT "platform_promotion_codes_max_redemptions_check" CHECK ((("max_redemptions" IS NULL) OR ("max_redemptions" > 0)))
);


ALTER TABLE "public"."platform_promotion_codes" OWNER TO "postgres";


COMMENT ON TABLE "public"."platform_promotion_codes" IS 'Synced copy of Stripe promotion codes. Links to coupons and optionally to specific customers.';



CREATE TABLE IF NOT EXISTS "public"."platform_staff" (
    "id" "uuid" DEFAULT "gen_random_uuid"() NOT NULL,
    "user_id" "uuid" NOT NULL,
    "created_at" timestamp with time zone DEFAULT "now"() NOT NULL,
    "created_by" "uuid"
);


ALTER TABLE "public"."platform_staff" OWNER TO "postgres";


COMMENT ON TABLE "public"."platform_staff" IS 'Platform-level admin users. First admin must be bootstrapped manually via SQL after deployment.';



CREATE TABLE IF NOT EXISTS "public"."platform_staff_permissions" (
    "id" "uuid" DEFAULT "gen_random_uuid"() NOT NULL,
    "user_id" "uuid" NOT NULL,
    "permission" "public"."platform_permission" NOT NULL,
    "granted_at" timestamp with time zone DEFAULT "now"() NOT NULL,
    "granted_by" "uuid"
);


ALTER TABLE "public"."platform_staff_permissions" OWNER TO "postgres";


COMMENT ON TABLE "public"."platform_staff_permissions" IS 'Junction table linking platform staff to their permissions. Permissions are hierarchical: admin includes all, billing_admin includes billing_support.';



CREATE TABLE IF NOT EXISTS "public"."report_versions" (
    "id" "uuid" DEFAULT "gen_random_uuid"() NOT NULL,
    "company_id" "uuid" NOT NULL,
    "inspection_id" "uuid" NOT NULL,
    "name" "text",
    "storage_path" "text" NOT NULL,
    "config" "jsonb",
    "file_size" integer,
    "created_at" timestamp with time zone DEFAULT "now"(),
    "created_by" "uuid"
);


ALTER TABLE "public"."report_versions" OWNER TO "postgres";


COMMENT ON TABLE "public"."report_versions" IS 'Stores PDF report version history for inspections';



COMMENT ON COLUMN "public"."report_versions"."name" IS 'Optional user-provided name (e.g., Final, Draft)';



COMMENT ON COLUMN "public"."report_versions"."storage_path" IS 'Path in Supabase storage: {company_id}/{inspection_id}/{version_id}.pdf';



COMMENT ON COLUMN "public"."report_versions"."config" IS 'ReportConfig snapshot used to generate this version';



COMMENT ON COLUMN "public"."report_versions"."file_size" IS 'PDF file size in bytes';



CREATE TABLE IF NOT EXISTS "public"."reports" (
    "id" "uuid" DEFAULT "extensions"."uuid_generate_v4"() NOT NULL,
    "company_id" "uuid" NOT NULL,
    "inspection_id" "uuid" NOT NULL,
    "title" "text",
    "summary" "text",
    "findings" "jsonb",
    "created_at" timestamp with time zone DEFAULT "now"() NOT NULL,
    "updated_at" timestamp with time zone DEFAULT "now"() NOT NULL,
    "content" "text" DEFAULT ''::"text" NOT NULL,
    "findings_storage_path" "text"
);


ALTER TABLE "public"."reports" OWNER TO "postgres";


CREATE TABLE IF NOT EXISTS "public"."sample_templates" (
    "id" "uuid" DEFAULT "gen_random_uuid"() NOT NULL,
    "name" "text" NOT NULL,
    "description" "text",
    "structure" "jsonb" DEFAULT '{"sections": []}'::"jsonb" NOT NULL,
    "sort_order" integer DEFAULT 0 NOT NULL,
    "is_active" boolean DEFAULT true NOT NULL,
    "created_at" timestamp with time zone DEFAULT "now"() NOT NULL,
    "updated_at" timestamp with time zone DEFAULT "now"() NOT NULL,
    "structure_storage_path" "text",
    "category" "text",
    "section_count" integer DEFAULT 0,
    "field_count" integer DEFAULT 0
);


ALTER TABLE "public"."sample_templates" OWNER TO "postgres";


CREATE TABLE IF NOT EXISTS "public"."subscription_invoices" (
    "id" "uuid" DEFAULT "gen_random_uuid"() NOT NULL,
    "company_id" "uuid" NOT NULL,
    "stripe_invoice_id" "text" NOT NULL,
    "amount_due" integer NOT NULL,
    "amount_paid" integer DEFAULT 0 NOT NULL,
    "status" "public"."invoice_status" DEFAULT 'draft'::"public"."invoice_status" NOT NULL,
    "invoice_url" "text",
    "invoice_pdf" "text",
    "period_start" timestamp with time zone,
    "period_end" timestamp with time zone,
    "created_at" timestamp with time zone DEFAULT "now"() NOT NULL
);


ALTER TABLE "public"."subscription_invoices" OWNER TO "postgres";


CREATE TABLE IF NOT EXISTS "public"."users" (
    "id" "uuid" NOT NULL,
    "email" "text" NOT NULL,
    "full_name" "text" NOT NULL,
    "company_id" "uuid" NOT NULL,
    "role" "public"."user_role" DEFAULT 'inspector'::"public"."user_role" NOT NULL,
    "created_at" timestamp with time zone DEFAULT "now"() NOT NULL,
    "updated_at" timestamp with time zone DEFAULT "now"() NOT NULL,
    "current_company_id" "uuid",
    "avatar_url" "text",
    "terms_accepted_at" timestamp with time zone
);


ALTER TABLE "public"."users" OWNER TO "postgres";


ALTER TABLE ONLY "public"."availability_schedules"
    ADD CONSTRAINT "availability_schedules_pkey" PRIMARY KEY ("id");



ALTER TABLE ONLY "public"."availability_schedules"
    ADD CONSTRAINT "availability_schedules_user_id_day_of_week_key" UNIQUE ("user_id", "day_of_week");



ALTER TABLE ONLY "public"."client_invitations"
    ADD CONSTRAINT "client_invitations_pkey" PRIMARY KEY ("id");



ALTER TABLE ONLY "public"."client_invitations"
    ADD CONSTRAINT "client_invitations_token_key" UNIQUE ("token");



ALTER TABLE ONLY "public"."clients"
    ADD CONSTRAINT "clients_pkey" PRIMARY KEY ("id");



ALTER TABLE ONLY "public"."companies"
    ADD CONSTRAINT "companies_pkey" PRIMARY KEY ("id");



ALTER TABLE ONLY "public"."companies"
    ADD CONSTRAINT "companies_slug_key" UNIQUE ("slug");



ALTER TABLE ONLY "public"."company_memberships"
    ADD CONSTRAINT "company_memberships_pkey" PRIMARY KEY ("id");



ALTER TABLE ONLY "public"."company_memberships"
    ADD CONSTRAINT "company_memberships_user_id_company_id_key" UNIQUE ("user_id", "company_id");



ALTER TABLE ONLY "public"."company_subscriptions"
    ADD CONSTRAINT "company_subscriptions_company_id_key" UNIQUE ("company_id");



ALTER TABLE ONLY "public"."company_subscriptions"
    ADD CONSTRAINT "company_subscriptions_pkey" PRIMARY KEY ("id");



ALTER TABLE ONLY "public"."feedback_attachments"
    ADD CONSTRAINT "feedback_attachments_pkey" PRIMARY KEY ("id");



ALTER TABLE ONLY "public"."feedback"
    ADD CONSTRAINT "feedback_pkey" PRIMARY KEY ("id");



ALTER TABLE ONLY "public"."google_calendar_connections"
    ADD CONSTRAINT "google_calendar_connections_pkey" PRIMARY KEY ("id");



ALTER TABLE ONLY "public"."google_calendar_connections"
    ADD CONSTRAINT "google_calendar_connections_user_id_key" UNIQUE ("user_id");



ALTER TABLE ONLY "public"."inspection_email_log"
    ADD CONSTRAINT "inspection_email_log_pkey" PRIMARY KEY ("id");



ALTER TABLE ONLY "public"."inspection_templates"
    ADD CONSTRAINT "inspection_templates_pkey" PRIMARY KEY ("id");



ALTER TABLE ONLY "public"."inspections"
    ADD CONSTRAINT "inspections_pkey" PRIMARY KEY ("id");



ALTER TABLE ONLY "public"."invitations"
    ADD CONSTRAINT "invitations_pkey" PRIMARY KEY ("id");



ALTER TABLE ONLY "public"."invitations"
    ADD CONSTRAINT "invitations_token_key" UNIQUE ("token");



ALTER TABLE ONLY "public"."llm_usage_logs"
    ADD CONSTRAINT "llm_usage_logs_pkey" PRIMARY KEY ("id");



ALTER TABLE ONLY "public"."platform_coupons"
    ADD CONSTRAINT "platform_coupons_pkey" PRIMARY KEY ("id");



ALTER TABLE ONLY "public"."platform_coupons"
    ADD CONSTRAINT "platform_coupons_stripe_coupon_id_key" UNIQUE ("stripe_coupon_id");



ALTER TABLE ONLY "public"."platform_promotion_codes"
    ADD CONSTRAINT "platform_promotion_codes_pkey" PRIMARY KEY ("id");



ALTER TABLE ONLY "public"."platform_promotion_codes"
    ADD CONSTRAINT "platform_promotion_codes_stripe_promotion_code_id_key" UNIQUE ("stripe_promotion_code_id");



ALTER TABLE ONLY "public"."platform_staff_permissions"
    ADD CONSTRAINT "platform_staff_permissions_pkey" PRIMARY KEY ("id");



ALTER TABLE ONLY "public"."platform_staff_permissions"
    ADD CONSTRAINT "platform_staff_permissions_user_id_permission_key" UNIQUE ("user_id", "permission");



ALTER TABLE ONLY "public"."platform_staff"
    ADD CONSTRAINT "platform_staff_pkey" PRIMARY KEY ("id");



ALTER TABLE ONLY "public"."platform_staff"
    ADD CONSTRAINT "platform_staff_user_id_key" UNIQUE ("user_id");



ALTER TABLE ONLY "public"."report_versions"
    ADD CONSTRAINT "report_versions_pkey" PRIMARY KEY ("id");



ALTER TABLE ONLY "public"."reports"
    ADD CONSTRAINT "reports_pkey" PRIMARY KEY ("id");



ALTER TABLE ONLY "public"."sample_templates"
    ADD CONSTRAINT "sample_templates_pkey" PRIMARY KEY ("id");



ALTER TABLE ONLY "public"."subscription_invoices"
    ADD CONSTRAINT "subscription_invoices_pkey" PRIMARY KEY ("id");



ALTER TABLE ONLY "public"."subscription_invoices"
    ADD CONSTRAINT "subscription_invoices_stripe_invoice_id_key" UNIQUE ("stripe_invoice_id");



ALTER TABLE ONLY "public"."users"
    ADD CONSTRAINT "users_pkey" PRIMARY KEY ("id");



CREATE INDEX "idx_availability_schedules_company_id" ON "public"."availability_schedules" USING "btree" ("company_id");



CREATE INDEX "idx_availability_schedules_user_id" ON "public"."availability_schedules" USING "btree" ("user_id");



CREATE INDEX "idx_client_invitations_client_id" ON "public"."client_invitations" USING "btree" ("client_id");



CREATE INDEX "idx_client_invitations_company_id" ON "public"."client_invitations" USING "btree" ("company_id");



CREATE INDEX "idx_client_invitations_invited_by" ON "public"."client_invitations" USING "btree" ("invited_by");



CREATE INDEX "idx_client_invitations_token" ON "public"."client_invitations" USING "btree" ("token");



CREATE INDEX "idx_clients_company_id" ON "public"."clients" USING "btree" ("company_id");



CREATE INDEX "idx_clients_status" ON "public"."clients" USING "btree" ("status");



CREATE INDEX "idx_clients_user_id" ON "public"."clients" USING "btree" ("user_id");



CREATE INDEX "idx_companies_slug" ON "public"."companies" USING "btree" ("slug");



CREATE INDEX "idx_company_subscriptions_company_id" ON "public"."company_subscriptions" USING "btree" ("company_id");



CREATE INDEX "idx_company_subscriptions_status" ON "public"."company_subscriptions" USING "btree" ("status");



CREATE INDEX "idx_company_subscriptions_stripe_customer_id" ON "public"."company_subscriptions" USING "btree" ("stripe_customer_id");



CREATE INDEX "idx_feedback_attachments_feedback_id" ON "public"."feedback_attachments" USING "btree" ("feedback_id");



CREATE INDEX "idx_feedback_company_id" ON "public"."feedback" USING "btree" ("company_id");



CREATE INDEX "idx_feedback_created_at" ON "public"."feedback" USING "btree" ("created_at" DESC);



CREATE INDEX "idx_feedback_status" ON "public"."feedback" USING "btree" ("status");



CREATE INDEX "idx_feedback_user_id" ON "public"."feedback" USING "btree" ("user_id");



CREATE INDEX "idx_google_calendar_connections_user_id" ON "public"."google_calendar_connections" USING "btree" ("user_id");



CREATE INDEX "idx_inspection_email_log_inspection_id" ON "public"."inspection_email_log" USING "btree" ("inspection_id");



CREATE INDEX "idx_inspection_templates_company_id" ON "public"."inspection_templates" USING "btree" ("company_id");



CREATE INDEX "idx_inspection_templates_is_active" ON "public"."inspection_templates" USING "btree" ("is_active");



CREATE INDEX "idx_inspections_client_id" ON "public"."inspections" USING "btree" ("client_id");



CREATE INDEX "idx_inspections_company_id" ON "public"."inspections" USING "btree" ("company_id");



CREATE INDEX "idx_inspections_data" ON "public"."inspections" USING "gin" ("data");



CREATE INDEX "idx_inspections_inspector_id" ON "public"."inspections" USING "btree" ("inspector_id");



CREATE INDEX "idx_inspections_report_generated_at" ON "public"."inspections" USING "btree" ("report_generated_at") WHERE ("report_generated_at" IS NOT NULL);



CREATE INDEX "idx_inspections_scheduled_date" ON "public"."inspections" USING "btree" ("scheduled_date");



CREATE INDEX "idx_inspections_template_id" ON "public"."inspections" USING "btree" ("template_id");



CREATE INDEX "idx_invitations_company" ON "public"."invitations" USING "btree" ("company_id");



CREATE INDEX "idx_invitations_email" ON "public"."invitations" USING "btree" ("email");



CREATE INDEX "idx_invitations_invited_by" ON "public"."invitations" USING "btree" ("invited_by");



CREATE INDEX "idx_invitations_token" ON "public"."invitations" USING "btree" ("token");



CREATE INDEX "idx_llm_usage_logs_company_id" ON "public"."llm_usage_logs" USING "btree" ("company_id") WHERE ("company_id" IS NOT NULL);



CREATE INDEX "idx_llm_usage_logs_created_at" ON "public"."llm_usage_logs" USING "btree" ("created_at" DESC);



CREATE INDEX "idx_llm_usage_logs_function_name" ON "public"."llm_usage_logs" USING "btree" ("function_name");



CREATE INDEX "idx_llm_usage_logs_model" ON "public"."llm_usage_logs" USING "btree" ("model");



CREATE INDEX "idx_memberships_company" ON "public"."company_memberships" USING "btree" ("company_id");



CREATE INDEX "idx_memberships_user" ON "public"."company_memberships" USING "btree" ("user_id");



CREATE INDEX "idx_platform_coupons_active" ON "public"."platform_coupons" USING "btree" ("is_active") WHERE ("is_active" = true);



CREATE INDEX "idx_platform_coupons_stripe_id" ON "public"."platform_coupons" USING "btree" ("stripe_coupon_id");



CREATE INDEX "idx_platform_promotion_codes_coupon" ON "public"."platform_promotion_codes" USING "btree" ("coupon_id");



CREATE INDEX "idx_platform_promotion_codes_customer" ON "public"."platform_promotion_codes" USING "btree" ("for_customer_id") WHERE ("for_customer_id" IS NOT NULL);



CREATE INDEX "idx_platform_promotion_codes_stripe_id" ON "public"."platform_promotion_codes" USING "btree" ("stripe_promotion_code_id");



CREATE INDEX "idx_platform_staff_permissions_permission" ON "public"."platform_staff_permissions" USING "btree" ("permission");



CREATE INDEX "idx_platform_staff_permissions_user_id" ON "public"."platform_staff_permissions" USING "btree" ("user_id");



CREATE INDEX "idx_platform_staff_user_id" ON "public"."platform_staff" USING "btree" ("user_id");



CREATE INDEX "idx_report_versions_company" ON "public"."report_versions" USING "btree" ("company_id");



CREATE INDEX "idx_report_versions_inspection" ON "public"."report_versions" USING "btree" ("inspection_id", "created_at" DESC);



CREATE INDEX "idx_reports_company_id" ON "public"."reports" USING "btree" ("company_id");



CREATE INDEX "idx_reports_inspection_id" ON "public"."reports" USING "btree" ("inspection_id");



CREATE INDEX "idx_subscription_invoices_company_id" ON "public"."subscription_invoices" USING "btree" ("company_id");



CREATE INDEX "idx_subscription_invoices_stripe_invoice_id" ON "public"."subscription_invoices" USING "btree" ("stripe_invoice_id");



CREATE INDEX "idx_users_company_id" ON "public"."users" USING "btree" ("company_id");



CREATE INDEX "idx_users_current_company_id" ON "public"."users" USING "btree" ("current_company_id");



CREATE OR REPLACE TRIGGER "create_trial_subscription_on_company_insert" AFTER INSERT ON "public"."companies" FOR EACH ROW EXECUTE FUNCTION "public"."create_trial_subscription_for_company"();



CREATE OR REPLACE TRIGGER "feedback_updated_at" BEFORE UPDATE ON "public"."feedback" FOR EACH ROW EXECUTE FUNCTION "public"."update_feedback_updated_at"();



CREATE OR REPLACE TRIGGER "sample_template_counts_trigger" BEFORE INSERT OR UPDATE OF "structure" ON "public"."sample_templates" FOR EACH ROW EXECUTE FUNCTION "public"."update_sample_template_counts"();



CREATE OR REPLACE TRIGGER "update_availability_schedules_updated_at" BEFORE UPDATE ON "public"."availability_schedules" FOR EACH ROW EXECUTE FUNCTION "public"."update_updated_at"();



CREATE OR REPLACE TRIGGER "update_client_invitations_updated_at" BEFORE UPDATE ON "public"."client_invitations" FOR EACH ROW EXECUTE FUNCTION "public"."update_updated_at"();



CREATE OR REPLACE TRIGGER "update_clients_updated_at" BEFORE UPDATE ON "public"."clients" FOR EACH ROW EXECUTE FUNCTION "public"."update_updated_at"();



CREATE OR REPLACE TRIGGER "update_companies_updated_at" BEFORE UPDATE ON "public"."companies" FOR EACH ROW EXECUTE FUNCTION "public"."update_updated_at"();



CREATE OR REPLACE TRIGGER "update_company_subscriptions_updated_at" BEFORE UPDATE ON "public"."company_subscriptions" FOR EACH ROW EXECUTE FUNCTION "public"."update_updated_at"();



CREATE OR REPLACE TRIGGER "update_google_calendar_connections_updated_at" BEFORE UPDATE ON "public"."google_calendar_connections" FOR EACH ROW EXECUTE FUNCTION "public"."update_updated_at"();



CREATE OR REPLACE TRIGGER "update_inspection_templates_updated_at" BEFORE UPDATE ON "public"."inspection_templates" FOR EACH ROW EXECUTE FUNCTION "public"."update_updated_at"();



CREATE OR REPLACE TRIGGER "update_inspections_updated_at" BEFORE UPDATE ON "public"."inspections" FOR EACH ROW EXECUTE FUNCTION "public"."update_updated_at"();



CREATE OR REPLACE TRIGGER "update_reports_updated_at" BEFORE UPDATE ON "public"."reports" FOR EACH ROW EXECUTE FUNCTION "public"."update_updated_at"();



CREATE OR REPLACE TRIGGER "update_sample_templates_updated_at" BEFORE UPDATE ON "public"."sample_templates" FOR EACH ROW EXECUTE FUNCTION "public"."update_updated_at"();



CREATE OR REPLACE TRIGGER "update_users_updated_at" BEFORE UPDATE ON "public"."users" FOR EACH ROW EXECUTE FUNCTION "public"."update_updated_at"();



ALTER TABLE ONLY "public"."availability_schedules"
    ADD CONSTRAINT "availability_schedules_company_id_fkey" FOREIGN KEY ("company_id") REFERENCES "public"."companies"("id") ON DELETE CASCADE;



ALTER TABLE ONLY "public"."availability_schedules"
    ADD CONSTRAINT "availability_schedules_user_id_fkey" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE CASCADE;



ALTER TABLE ONLY "public"."client_invitations"
    ADD CONSTRAINT "client_invitations_client_id_fkey" FOREIGN KEY ("client_id") REFERENCES "public"."clients"("id") ON DELETE CASCADE;



ALTER TABLE ONLY "public"."client_invitations"
    ADD CONSTRAINT "client_invitations_company_id_fkey" FOREIGN KEY ("company_id") REFERENCES "public"."companies"("id") ON DELETE CASCADE;



ALTER TABLE ONLY "public"."client_invitations"
    ADD CONSTRAINT "client_invitations_invited_by_fkey" FOREIGN KEY ("invited_by") REFERENCES "public"."users"("id") ON DELETE SET NULL;



ALTER TABLE ONLY "public"."clients"
    ADD CONSTRAINT "clients_company_id_fkey" FOREIGN KEY ("company_id") REFERENCES "public"."companies"("id") ON DELETE CASCADE;



ALTER TABLE ONLY "public"."clients"
    ADD CONSTRAINT "clients_user_id_fkey" FOREIGN KEY ("user_id") REFERENCES "auth"."users"("id") ON DELETE SET NULL;



ALTER TABLE ONLY "public"."company_memberships"
    ADD CONSTRAINT "company_memberships_company_id_fkey" FOREIGN KEY ("company_id") REFERENCES "public"."companies"("id") ON DELETE CASCADE;



ALTER TABLE ONLY "public"."company_memberships"
    ADD CONSTRAINT "company_memberships_user_id_fkey" FOREIGN KEY ("user_id") REFERENCES "auth"."users"("id") ON DELETE CASCADE;



ALTER TABLE ONLY "public"."company_subscriptions"
    ADD CONSTRAINT "company_subscriptions_company_id_fkey" FOREIGN KEY ("company_id") REFERENCES "public"."companies"("id") ON DELETE CASCADE;



ALTER TABLE ONLY "public"."feedback_attachments"
    ADD CONSTRAINT "feedback_attachments_feedback_id_fkey" FOREIGN KEY ("feedback_id") REFERENCES "public"."feedback"("id") ON DELETE CASCADE;



ALTER TABLE ONLY "public"."feedback"
    ADD CONSTRAINT "feedback_company_id_fkey" FOREIGN KEY ("company_id") REFERENCES "public"."companies"("id") ON DELETE SET NULL;



ALTER TABLE ONLY "public"."feedback"
    ADD CONSTRAINT "feedback_user_id_fkey" FOREIGN KEY ("user_id") REFERENCES "auth"."users"("id") ON DELETE CASCADE;



ALTER TABLE ONLY "public"."google_calendar_connections"
    ADD CONSTRAINT "google_calendar_connections_user_id_fkey" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE CASCADE;



ALTER TABLE ONLY "public"."inspection_email_log"
    ADD CONSTRAINT "inspection_email_log_inspection_id_fkey" FOREIGN KEY ("inspection_id") REFERENCES "public"."inspections"("id") ON DELETE CASCADE;



ALTER TABLE ONLY "public"."inspection_templates"
    ADD CONSTRAINT "inspection_templates_company_id_fkey" FOREIGN KEY ("company_id") REFERENCES "public"."companies"("id") ON DELETE CASCADE;



ALTER TABLE ONLY "public"."inspections"
    ADD CONSTRAINT "inspections_client_id_fkey" FOREIGN KEY ("client_id") REFERENCES "public"."clients"("id") ON DELETE CASCADE;



ALTER TABLE ONLY "public"."inspections"
    ADD CONSTRAINT "inspections_company_id_fkey" FOREIGN KEY ("company_id") REFERENCES "public"."companies"("id") ON DELETE CASCADE;



ALTER TABLE ONLY "public"."inspections"
    ADD CONSTRAINT "inspections_inspector_id_fkey" FOREIGN KEY ("inspector_id") REFERENCES "public"."users"("id") ON DELETE CASCADE;



ALTER TABLE ONLY "public"."inspections"
    ADD CONSTRAINT "inspections_template_id_fkey" FOREIGN KEY ("template_id") REFERENCES "public"."inspection_templates"("id") ON DELETE SET NULL;



ALTER TABLE ONLY "public"."invitations"
    ADD CONSTRAINT "invitations_company_id_fkey" FOREIGN KEY ("company_id") REFERENCES "public"."companies"("id") ON DELETE CASCADE;



ALTER TABLE ONLY "public"."invitations"
    ADD CONSTRAINT "invitations_invited_by_fkey" FOREIGN KEY ("invited_by") REFERENCES "auth"."users"("id") ON DELETE SET NULL;



ALTER TABLE ONLY "public"."llm_usage_logs"
    ADD CONSTRAINT "llm_usage_logs_company_id_fkey" FOREIGN KEY ("company_id") REFERENCES "public"."companies"("id") ON DELETE SET NULL;



ALTER TABLE ONLY "public"."llm_usage_logs"
    ADD CONSTRAINT "llm_usage_logs_user_id_fkey" FOREIGN KEY ("user_id") REFERENCES "auth"."users"("id") ON DELETE SET NULL;



ALTER TABLE ONLY "public"."platform_promotion_codes"
    ADD CONSTRAINT "platform_promotion_codes_coupon_id_fkey" FOREIGN KEY ("coupon_id") REFERENCES "public"."platform_coupons"("id") ON DELETE CASCADE;



ALTER TABLE ONLY "public"."platform_promotion_codes"
    ADD CONSTRAINT "platform_promotion_codes_for_customer_id_fkey" FOREIGN KEY ("for_customer_id") REFERENCES "public"."companies"("id") ON DELETE SET NULL;



ALTER TABLE ONLY "public"."platform_staff"
    ADD CONSTRAINT "platform_staff_created_by_fkey" FOREIGN KEY ("created_by") REFERENCES "auth"."users"("id") ON DELETE SET NULL;



ALTER TABLE ONLY "public"."platform_staff_permissions"
    ADD CONSTRAINT "platform_staff_permissions_granted_by_fkey" FOREIGN KEY ("granted_by") REFERENCES "auth"."users"("id") ON DELETE SET NULL;



ALTER TABLE ONLY "public"."platform_staff_permissions"
    ADD CONSTRAINT "platform_staff_permissions_user_id_fkey" FOREIGN KEY ("user_id") REFERENCES "public"."platform_staff"("user_id") ON DELETE CASCADE;



ALTER TABLE ONLY "public"."platform_staff"
    ADD CONSTRAINT "platform_staff_user_id_fkey" FOREIGN KEY ("user_id") REFERENCES "auth"."users"("id") ON DELETE CASCADE;



ALTER TABLE ONLY "public"."report_versions"
    ADD CONSTRAINT "report_versions_company_id_fkey" FOREIGN KEY ("company_id") REFERENCES "public"."companies"("id") ON DELETE CASCADE;



ALTER TABLE ONLY "public"."report_versions"
    ADD CONSTRAINT "report_versions_created_by_fkey" FOREIGN KEY ("created_by") REFERENCES "public"."users"("id") ON DELETE SET NULL;



ALTER TABLE ONLY "public"."report_versions"
    ADD CONSTRAINT "report_versions_inspection_id_fkey" FOREIGN KEY ("inspection_id") REFERENCES "public"."inspections"("id") ON DELETE CASCADE;



ALTER TABLE ONLY "public"."reports"
    ADD CONSTRAINT "reports_company_id_fkey" FOREIGN KEY ("company_id") REFERENCES "public"."companies"("id") ON DELETE CASCADE;



ALTER TABLE ONLY "public"."reports"
    ADD CONSTRAINT "reports_inspection_id_fkey" FOREIGN KEY ("inspection_id") REFERENCES "public"."inspections"("id") ON DELETE CASCADE;



ALTER TABLE ONLY "public"."subscription_invoices"
    ADD CONSTRAINT "subscription_invoices_company_id_fkey" FOREIGN KEY ("company_id") REFERENCES "public"."companies"("id") ON DELETE CASCADE;



ALTER TABLE ONLY "public"."users"
    ADD CONSTRAINT "users_company_id_fkey" FOREIGN KEY ("company_id") REFERENCES "public"."companies"("id") ON DELETE CASCADE;



ALTER TABLE ONLY "public"."users"
    ADD CONSTRAINT "users_current_company_id_fkey" FOREIGN KEY ("current_company_id") REFERENCES "public"."companies"("id") ON DELETE SET NULL;



ALTER TABLE ONLY "public"."users"
    ADD CONSTRAINT "users_id_fkey" FOREIGN KEY ("id") REFERENCES "auth"."users"("id") ON DELETE CASCADE;



CREATE POLICY "Admins can create invitations" ON "public"."invitations" FOR INSERT WITH CHECK ((("company_id" = "public"."get_user_company_id"()) AND "public"."is_admin"()));



CREATE POLICY "Admins can delete company clients" ON "public"."clients" FOR DELETE USING ((("company_id" = "public"."get_user_company_id"()) AND "public"."is_admin"()));



CREATE POLICY "Admins can delete company inspections" ON "public"."inspections" FOR DELETE USING ((("company_id" = "public"."get_user_company_id"()) AND "public"."is_admin"()));



CREATE POLICY "Admins can delete company members" ON "public"."users" FOR DELETE USING ((("company_id" = "public"."get_user_company_id"()) AND ("id" <> ( SELECT "auth"."uid"() AS "uid")) AND "public"."is_admin"()));



CREATE POLICY "Admins can delete company reports" ON "public"."reports" FOR DELETE USING ((("company_id" = "public"."get_user_company_id"()) AND "public"."is_admin"()));



CREATE POLICY "Admins can delete company templates" ON "public"."inspection_templates" FOR DELETE USING ((("company_id" = "public"."get_user_company_id"()) AND ("public"."get_user_role"() = 'admin'::"public"."user_role")));



CREATE POLICY "Admins can insert company templates" ON "public"."inspection_templates" FOR INSERT WITH CHECK ((("company_id" = "public"."get_user_company_id"()) AND ("public"."get_user_role"() = 'admin'::"public"."user_role")));



CREATE POLICY "Admins can remove members" ON "public"."company_memberships" FOR DELETE USING ((("company_id" = "public"."get_user_company_id"()) AND "public"."is_admin"() AND ("user_id" <> ( SELECT "auth"."uid"() AS "uid"))));



CREATE POLICY "Admins can update company templates" ON "public"."inspection_templates" FOR UPDATE USING ((("company_id" = "public"."get_user_company_id"()) AND ("public"."get_user_role"() = 'admin'::"public"."user_role")));



CREATE POLICY "Admins can update invitations" ON "public"."invitations" FOR UPDATE USING ((("company_id" = "public"."get_user_company_id"()) AND "public"."is_admin"()));



CREATE POLICY "Admins can update own company" ON "public"."companies" FOR UPDATE USING ((("id" = "public"."get_user_company_id"()) AND "public"."is_admin"()));



CREATE POLICY "Admins can view company invoices" ON "public"."subscription_invoices" FOR SELECT TO "authenticated" USING ((("company_id" = "public"."get_user_company_id"()) AND (EXISTS ( SELECT 1
   FROM "public"."users"
  WHERE (("users"."id" = "auth"."uid"()) AND ("users"."role" = 'admin'::"public"."user_role"))))));



CREATE POLICY "Admins can view invitations" ON "public"."invitations" FOR SELECT USING ((("company_id" = "public"."get_user_company_id"()) AND "public"."is_admin"()));



CREATE POLICY "Anyone can lookup invitation by token" ON "public"."client_invitations" FOR SELECT USING ((("token" IS NOT NULL) AND ("status" = 'pending'::"text")));



CREATE POLICY "Authenticated users can view active samples" ON "public"."sample_templates" FOR SELECT USING (("is_active" = true));



CREATE POLICY "Clients can update own profile" ON "public"."clients" FOR UPDATE USING (("user_id" = "auth"."uid"())) WITH CHECK (("user_id" = "auth"."uid"()));



CREATE POLICY "Platform admins can delete permissions" ON "public"."platform_staff_permissions" FOR DELETE USING ("public"."has_platform_permission"('admin'::"public"."platform_permission"));



CREATE POLICY "Platform admins can delete staff" ON "public"."platform_staff" FOR DELETE USING ("public"."has_platform_permission"('admin'::"public"."platform_permission"));



CREATE POLICY "Platform admins can insert permissions" ON "public"."platform_staff_permissions" FOR INSERT WITH CHECK ("public"."has_platform_permission"('admin'::"public"."platform_permission"));



CREATE POLICY "Platform admins can insert staff" ON "public"."platform_staff" FOR INSERT WITH CHECK ("public"."has_platform_permission"('admin'::"public"."platform_permission"));



CREATE POLICY "Platform admins can update permissions" ON "public"."platform_staff_permissions" FOR UPDATE USING ("public"."has_platform_permission"('admin'::"public"."platform_permission"));



CREATE POLICY "Platform admins can update staff" ON "public"."platform_staff" FOR UPDATE USING ("public"."has_platform_permission"('admin'::"public"."platform_permission"));



CREATE POLICY "Platform admins can view all permissions" ON "public"."platform_staff_permissions" FOR SELECT USING ("public"."has_platform_permission"('admin'::"public"."platform_permission"));



CREATE POLICY "Platform admins can view all staff" ON "public"."platform_staff" FOR SELECT USING ("public"."has_platform_permission"('admin'::"public"."platform_permission"));



CREATE POLICY "Platform staff can delete attachments" ON "public"."feedback_attachments" FOR DELETE USING ("public"."has_platform_permission"('view_feedback'::"public"."platform_permission"));



CREATE POLICY "Platform staff can delete coupons" ON "public"."platform_coupons" FOR DELETE USING ("public"."has_platform_permission"('manage_promotions'::"public"."platform_permission"));



CREATE POLICY "Platform staff can delete feedback" ON "public"."feedback" FOR DELETE USING ("public"."has_platform_permission"('view_feedback'::"public"."platform_permission"));



CREATE POLICY "Platform staff can delete promotion codes" ON "public"."platform_promotion_codes" FOR DELETE USING ("public"."has_platform_permission"('manage_promotions'::"public"."platform_permission"));



CREATE POLICY "Platform staff can insert coupons" ON "public"."platform_coupons" FOR INSERT WITH CHECK ("public"."has_platform_permission"('manage_promotions'::"public"."platform_permission"));



CREATE POLICY "Platform staff can insert promotion codes" ON "public"."platform_promotion_codes" FOR INSERT WITH CHECK ("public"."has_platform_permission"('manage_promotions'::"public"."platform_permission"));



CREATE POLICY "Platform staff can manage sample templates" ON "public"."sample_templates" USING ("public"."has_platform_permission"('edit_inspection_template_samples'::"public"."platform_permission")) WITH CHECK ("public"."has_platform_permission"('edit_inspection_template_samples'::"public"."platform_permission"));



CREATE POLICY "Platform staff can manage samples" ON "public"."sample_templates" USING (("public"."has_platform_permission"('edit_inspection_template_samples'::"public"."platform_permission") OR "public"."has_platform_permission"('edit_report_template_samples'::"public"."platform_permission"))) WITH CHECK (("public"."has_platform_permission"('edit_inspection_template_samples'::"public"."platform_permission") OR "public"."has_platform_permission"('edit_report_template_samples'::"public"."platform_permission")));



CREATE POLICY "Platform staff can update coupons" ON "public"."platform_coupons" FOR UPDATE USING ("public"."has_platform_permission"('manage_promotions'::"public"."platform_permission"));



CREATE POLICY "Platform staff can update feedback" ON "public"."feedback" FOR UPDATE USING ("public"."has_platform_permission"('view_feedback'::"public"."platform_permission"));



CREATE POLICY "Platform staff can update promotion codes" ON "public"."platform_promotion_codes" FOR UPDATE USING ("public"."has_platform_permission"('manage_promotions'::"public"."platform_permission"));



CREATE POLICY "Platform staff can view all attachments" ON "public"."feedback_attachments" FOR SELECT USING ("public"."has_platform_permission"('view_feedback'::"public"."platform_permission"));



CREATE POLICY "Platform staff can view all feedback" ON "public"."feedback" FOR SELECT USING ("public"."has_platform_permission"('view_feedback'::"public"."platform_permission"));



CREATE POLICY "Platform staff can view coupons" ON "public"."platform_coupons" FOR SELECT USING ("public"."has_platform_permission"('manage_promotions'::"public"."platform_permission"));



CREATE POLICY "Platform staff can view promotion codes" ON "public"."platform_promotion_codes" FOR SELECT USING ("public"."has_platform_permission"('manage_promotions'::"public"."platform_permission"));



CREATE POLICY "Service role can insert llm_usage_logs" ON "public"."llm_usage_logs" FOR INSERT TO "service_role" WITH CHECK (true);



CREATE POLICY "Service role can manage invoices" ON "public"."subscription_invoices" TO "service_role" USING (true) WITH CHECK (true);



CREATE POLICY "Service role can manage subscriptions" ON "public"."company_subscriptions" TO "service_role" USING (true) WITH CHECK (true);



CREATE POLICY "Service role can select llm_usage_logs" ON "public"."llm_usage_logs" FOR SELECT TO "service_role" USING (true);



CREATE POLICY "Staff can create client invitations" ON "public"."client_invitations" FOR INSERT WITH CHECK (("company_id" = "public"."get_user_company_id"()));



CREATE POLICY "Staff can create company clients" ON "public"."clients" FOR INSERT WITH CHECK ((("company_id" = "public"."get_user_company_id"()) AND "public"."is_staff"()));



CREATE POLICY "Staff can create company reports" ON "public"."reports" FOR INSERT WITH CHECK ((("company_id" = "public"."get_user_company_id"()) AND "public"."is_staff"()));



CREATE POLICY "Staff can insert email logs" ON "public"."inspection_email_log" FOR INSERT WITH CHECK ((EXISTS ( SELECT 1
   FROM ("public"."inspections" "i"
     JOIN "public"."company_memberships" "cm" ON (("cm"."company_id" = "i"."company_id")))
  WHERE (("i"."id" = "inspection_email_log"."inspection_id") AND ("cm"."user_id" = ( SELECT "auth"."uid"() AS "uid")) AND ("cm"."role" = ANY (ARRAY['admin'::"public"."user_role", 'inspector'::"public"."user_role"]))))));



CREATE POLICY "Staff can update client invitations" ON "public"."client_invitations" FOR UPDATE USING (("company_id" = "public"."get_user_company_id"()));



CREATE POLICY "Staff can update company clients" ON "public"."clients" FOR UPDATE USING ((("company_id" = "public"."get_user_company_id"()) AND "public"."is_staff"())) WITH CHECK ((("company_id" = "public"."get_user_company_id"()) AND "public"."is_staff"()));



CREATE POLICY "Staff can update company inspections" ON "public"."inspections" FOR UPDATE USING ((("company_id" = "public"."get_user_company_id"()) AND "public"."is_staff"()));



CREATE POLICY "Staff can update company reports" ON "public"."reports" FOR UPDATE USING ((("company_id" = "public"."get_user_company_id"()) AND "public"."is_staff"()));



CREATE POLICY "Staff can view company client invitations" ON "public"."client_invitations" FOR SELECT USING (("company_id" = "public"."get_user_company_id"()));



CREATE POLICY "Staff can view company subscription" ON "public"."company_subscriptions" FOR SELECT TO "authenticated" USING ((("company_id" = "public"."get_user_company_id"()) AND (EXISTS ( SELECT 1
   FROM "public"."users"
  WHERE (("users"."id" = "auth"."uid"()) AND ("users"."role" = ANY (ARRAY['admin'::"public"."user_role", 'inspector'::"public"."user_role"])))))));



CREATE POLICY "Staff can view email logs" ON "public"."inspection_email_log" FOR SELECT USING ((EXISTS ( SELECT 1
   FROM ("public"."inspections" "i"
     JOIN "public"."company_memberships" "cm" ON (("cm"."company_id" = "i"."company_id")))
  WHERE (("i"."id" = "inspection_email_log"."inspection_id") AND ("cm"."user_id" = ( SELECT "auth"."uid"() AS "uid")) AND ("cm"."role" = ANY (ARRAY['admin'::"public"."user_role", 'inspector'::"public"."user_role"]))))));



CREATE POLICY "Users can access their company's report versions" ON "public"."report_versions" USING (("company_id" = "public"."get_user_company_id"()));



CREATE POLICY "Users can create inspections" ON "public"."inspections" FOR INSERT WITH CHECK (((("company_id" = "public"."get_user_company_id"()) AND "public"."is_staff"()) OR (("client_id" IN ( SELECT "clients"."id"
   FROM "public"."clients"
  WHERE ("clients"."user_id" = ( SELECT "auth"."uid"() AS "uid")))) AND ("status" = 'requested'::"public"."inspection_status"))));



CREATE POLICY "Users can create own profile" ON "public"."users" FOR INSERT WITH CHECK (("id" = ( SELECT "auth"."uid"() AS "uid")));



CREATE POLICY "Users can insert own attachments" ON "public"."feedback_attachments" FOR INSERT WITH CHECK ((EXISTS ( SELECT 1
   FROM "public"."feedback"
  WHERE (("feedback"."id" = "feedback_attachments"."feedback_id") AND ("feedback"."user_id" = "auth"."uid"())))));



CREATE POLICY "Users can insert own feedback" ON "public"."feedback" FOR INSERT WITH CHECK (("auth"."uid"() = "user_id"));



CREATE POLICY "Users can manage own availability" ON "public"."availability_schedules" USING (("user_id" = ( SELECT "auth"."uid"() AS "uid")));



CREATE POLICY "Users can manage own google connection" ON "public"."google_calendar_connections" USING (("user_id" = ( SELECT "auth"."uid"() AS "uid")));



CREATE POLICY "Users can update profiles" ON "public"."users" FOR UPDATE USING ((("id" = ( SELECT "auth"."uid"() AS "uid")) OR (("company_id" = "public"."get_user_company_id"()) AND "public"."is_admin"())));



CREATE POLICY "Users can view availability" ON "public"."availability_schedules" FOR SELECT USING ((("user_id" = ( SELECT "auth"."uid"() AS "uid")) OR ("company_id" = "public"."get_user_company_id"())));



CREATE POLICY "Users can view clients" ON "public"."clients" FOR SELECT USING ((("user_id" = ( SELECT "auth"."uid"() AS "uid")) OR (("company_id" = "public"."get_user_company_id"()) AND "public"."is_staff"())));



CREATE POLICY "Users can view company templates" ON "public"."inspection_templates" FOR SELECT USING (("company_id" = "public"."get_user_company_id"()));



CREATE POLICY "Users can view inspections" ON "public"."inspections" FOR SELECT USING ((("client_id" IN ( SELECT "clients"."id"
   FROM "public"."clients"
  WHERE ("clients"."user_id" = ( SELECT "auth"."uid"() AS "uid")))) OR (("company_id" = "public"."get_user_company_id"()) AND "public"."is_staff"())));



CREATE POLICY "Users can view memberships" ON "public"."company_memberships" FOR SELECT USING ((("user_id" = ( SELECT "auth"."uid"() AS "uid")) OR ("company_id" = "public"."get_user_company_id"())));



CREATE POLICY "Users can view own attachments" ON "public"."feedback_attachments" FOR SELECT USING ((EXISTS ( SELECT 1
   FROM "public"."feedback"
  WHERE (("feedback"."id" = "feedback_attachments"."feedback_id") AND ("feedback"."user_id" = "auth"."uid"())))));



CREATE POLICY "Users can view own company" ON "public"."companies" FOR SELECT USING (("id" = "public"."get_user_company_id"()));



CREATE POLICY "Users can view own feedback" ON "public"."feedback" FOR SELECT USING (("auth"."uid"() = "user_id"));



CREATE POLICY "Users can view own permissions" ON "public"."platform_staff_permissions" FOR SELECT USING (("user_id" = "auth"."uid"()));



CREATE POLICY "Users can view own staff record" ON "public"."platform_staff" FOR SELECT USING (("user_id" = "auth"."uid"()));



CREATE POLICY "Users can view profiles" ON "public"."users" FOR SELECT USING ((("id" = ( SELECT "auth"."uid"() AS "uid")) OR (("company_id" = "public"."get_user_company_id"()) AND "public"."is_staff"()) OR ("id" IN ( SELECT "public"."get_client_inspector_ids"() AS "get_client_inspector_ids"))));



CREATE POLICY "Users can view reports" ON "public"."reports" FOR SELECT USING ((("inspection_id" IN ( SELECT "i"."id"
   FROM ("public"."inspections" "i"
     JOIN "public"."clients" "c" ON (("i"."client_id" = "c"."id")))
  WHERE ("c"."user_id" = ( SELECT "auth"."uid"() AS "uid")))) OR (("company_id" = "public"."get_user_company_id"()) AND "public"."is_staff"())));



ALTER TABLE "public"."availability_schedules" ENABLE ROW LEVEL SECURITY;


ALTER TABLE "public"."client_invitations" ENABLE ROW LEVEL SECURITY;


ALTER TABLE "public"."clients" ENABLE ROW LEVEL SECURITY;


ALTER TABLE "public"."companies" ENABLE ROW LEVEL SECURITY;


ALTER TABLE "public"."company_memberships" ENABLE ROW LEVEL SECURITY;


ALTER TABLE "public"."company_subscriptions" ENABLE ROW LEVEL SECURITY;


ALTER TABLE "public"."feedback" ENABLE ROW LEVEL SECURITY;


ALTER TABLE "public"."feedback_attachments" ENABLE ROW LEVEL SECURITY;


ALTER TABLE "public"."google_calendar_connections" ENABLE ROW LEVEL SECURITY;


ALTER TABLE "public"."inspection_email_log" ENABLE ROW LEVEL SECURITY;


ALTER TABLE "public"."inspection_templates" ENABLE ROW LEVEL SECURITY;


ALTER TABLE "public"."inspections" ENABLE ROW LEVEL SECURITY;


ALTER TABLE "public"."invitations" ENABLE ROW LEVEL SECURITY;


ALTER TABLE "public"."llm_usage_logs" ENABLE ROW LEVEL SECURITY;


ALTER TABLE "public"."platform_coupons" ENABLE ROW LEVEL SECURITY;


ALTER TABLE "public"."platform_promotion_codes" ENABLE ROW LEVEL SECURITY;


ALTER TABLE "public"."platform_staff" ENABLE ROW LEVEL SECURITY;


ALTER TABLE "public"."platform_staff_permissions" ENABLE ROW LEVEL SECURITY;


ALTER TABLE "public"."report_versions" ENABLE ROW LEVEL SECURITY;


ALTER TABLE "public"."reports" ENABLE ROW LEVEL SECURITY;


ALTER TABLE "public"."sample_templates" ENABLE ROW LEVEL SECURITY;


ALTER TABLE "public"."subscription_invoices" ENABLE ROW LEVEL SECURITY;


ALTER TABLE "public"."users" ENABLE ROW LEVEL SECURITY;


GRANT USAGE ON SCHEMA "public" TO "postgres";
GRANT USAGE ON SCHEMA "public" TO "anon";
GRANT USAGE ON SCHEMA "public" TO "authenticated";
GRANT USAGE ON SCHEMA "public" TO "service_role";



GRANT ALL ON FUNCTION "public"."accept_client_invitation"("p_token" "text", "p_full_name" "text") TO "anon";
GRANT ALL ON FUNCTION "public"."accept_client_invitation"("p_token" "text", "p_full_name" "text") TO "authenticated";
GRANT ALL ON FUNCTION "public"."accept_client_invitation"("p_token" "text", "p_full_name" "text") TO "service_role";



GRANT ALL ON FUNCTION "public"."accept_client_invitation"("p_token" "text", "p_user_id" "uuid", "p_full_name" "text") TO "anon";
GRANT ALL ON FUNCTION "public"."accept_client_invitation"("p_token" "text", "p_user_id" "uuid", "p_full_name" "text") TO "authenticated";
GRANT ALL ON FUNCTION "public"."accept_client_invitation"("p_token" "text", "p_user_id" "uuid", "p_full_name" "text") TO "service_role";



GRANT ALL ON FUNCTION "public"."accept_client_invitation"("p_token" "text", "p_user_id" "uuid", "p_full_name" "text", "p_terms_accepted" boolean) TO "anon";
GRANT ALL ON FUNCTION "public"."accept_client_invitation"("p_token" "text", "p_user_id" "uuid", "p_full_name" "text", "p_terms_accepted" boolean) TO "authenticated";
GRANT ALL ON FUNCTION "public"."accept_client_invitation"("p_token" "text", "p_user_id" "uuid", "p_full_name" "text", "p_terms_accepted" boolean) TO "service_role";



GRANT ALL ON FUNCTION "public"."accept_invitation"("p_token" "text", "p_full_name" "text") TO "anon";
GRANT ALL ON FUNCTION "public"."accept_invitation"("p_token" "text", "p_full_name" "text") TO "authenticated";
GRANT ALL ON FUNCTION "public"."accept_invitation"("p_token" "text", "p_full_name" "text") TO "service_role";



GRANT ALL ON FUNCTION "public"."accept_terms"() TO "anon";
GRANT ALL ON FUNCTION "public"."accept_terms"() TO "authenticated";
GRANT ALL ON FUNCTION "public"."accept_terms"() TO "service_role";



GRANT ALL ON FUNCTION "public"."assign_inspector"("p_inspection_id" "uuid", "p_inspector_id" "uuid") TO "anon";
GRANT ALL ON FUNCTION "public"."assign_inspector"("p_inspection_id" "uuid", "p_inspector_id" "uuid") TO "authenticated";
GRANT ALL ON FUNCTION "public"."assign_inspector"("p_inspection_id" "uuid", "p_inspector_id" "uuid") TO "service_role";



GRANT ALL ON FUNCTION "public"."can_access_inspection_data_storage"("object_path" "text") TO "anon";
GRANT ALL ON FUNCTION "public"."can_access_inspection_data_storage"("object_path" "text") TO "authenticated";
GRANT ALL ON FUNCTION "public"."can_access_inspection_data_storage"("object_path" "text") TO "service_role";



GRANT ALL ON FUNCTION "public"."can_access_inspection_media"("object_path" "text") TO "anon";
GRANT ALL ON FUNCTION "public"."can_access_inspection_media"("object_path" "text") TO "authenticated";
GRANT ALL ON FUNCTION "public"."can_access_inspection_media"("object_path" "text") TO "service_role";



GRANT ALL ON FUNCTION "public"."can_access_report_storage"("object_path" "text") TO "anon";
GRANT ALL ON FUNCTION "public"."can_access_report_storage"("object_path" "text") TO "authenticated";
GRANT ALL ON FUNCTION "public"."can_access_report_storage"("object_path" "text") TO "service_role";



GRANT ALL ON FUNCTION "public"."can_access_template_storage"("object_path" "text") TO "anon";
GRANT ALL ON FUNCTION "public"."can_access_template_storage"("object_path" "text") TO "authenticated";
GRANT ALL ON FUNCTION "public"."can_access_template_storage"("object_path" "text") TO "service_role";



GRANT ALL ON FUNCTION "public"."can_upload_company_logo"("file_path" "text") TO "anon";
GRANT ALL ON FUNCTION "public"."can_upload_company_logo"("file_path" "text") TO "authenticated";
GRANT ALL ON FUNCTION "public"."can_upload_company_logo"("file_path" "text") TO "service_role";



GRANT ALL ON FUNCTION "public"."can_upload_inspection_media"("object_path" "text") TO "anon";
GRANT ALL ON FUNCTION "public"."can_upload_inspection_media"("object_path" "text") TO "authenticated";
GRANT ALL ON FUNCTION "public"."can_upload_inspection_media"("object_path" "text") TO "service_role";



GRANT ALL ON FUNCTION "public"."can_write_inspection_data_storage"("object_path" "text") TO "anon";
GRANT ALL ON FUNCTION "public"."can_write_inspection_data_storage"("object_path" "text") TO "authenticated";
GRANT ALL ON FUNCTION "public"."can_write_inspection_data_storage"("object_path" "text") TO "service_role";



GRANT ALL ON FUNCTION "public"."can_write_report_storage"("object_path" "text") TO "anon";
GRANT ALL ON FUNCTION "public"."can_write_report_storage"("object_path" "text") TO "authenticated";
GRANT ALL ON FUNCTION "public"."can_write_report_storage"("object_path" "text") TO "service_role";



GRANT ALL ON FUNCTION "public"."can_write_template_storage"("object_path" "text") TO "anon";
GRANT ALL ON FUNCTION "public"."can_write_template_storage"("object_path" "text") TO "authenticated";
GRANT ALL ON FUNCTION "public"."can_write_template_storage"("object_path" "text") TO "service_role";



GRANT ALL ON FUNCTION "public"."count_template_structure"("structure" "jsonb") TO "anon";
GRANT ALL ON FUNCTION "public"."count_template_structure"("structure" "jsonb") TO "authenticated";
GRANT ALL ON FUNCTION "public"."count_template_structure"("structure" "jsonb") TO "service_role";



GRANT ALL ON FUNCTION "public"."create_booking"("p_company_slug" "text", "p_scheduled_date" timestamp with time zone, "p_duration_minutes" integer, "p_inspector_id" "uuid", "p_property_address" "text", "p_client_name" "text", "p_client_email" "text", "p_client_phone" "text", "p_client_type" "text", "p_notes" "text") TO "anon";
GRANT ALL ON FUNCTION "public"."create_booking"("p_company_slug" "text", "p_scheduled_date" timestamp with time zone, "p_duration_minutes" integer, "p_inspector_id" "uuid", "p_property_address" "text", "p_client_name" "text", "p_client_email" "text", "p_client_phone" "text", "p_client_type" "text", "p_notes" "text") TO "authenticated";
GRANT ALL ON FUNCTION "public"."create_booking"("p_company_slug" "text", "p_scheduled_date" timestamp with time zone, "p_duration_minutes" integer, "p_inspector_id" "uuid", "p_property_address" "text", "p_client_name" "text", "p_client_email" "text", "p_client_phone" "text", "p_client_type" "text", "p_notes" "text") TO "service_role";



GRANT ALL ON FUNCTION "public"."create_company_and_user"("user_id" "uuid", "user_email" "text", "user_full_name" "text", "company_name" "text") TO "anon";
GRANT ALL ON FUNCTION "public"."create_company_and_user"("user_id" "uuid", "user_email" "text", "user_full_name" "text", "company_name" "text") TO "authenticated";
GRANT ALL ON FUNCTION "public"."create_company_and_user"("user_id" "uuid", "user_email" "text", "user_full_name" "text", "company_name" "text") TO "service_role";



GRANT ALL ON FUNCTION "public"."create_company_and_user"("user_id" "uuid", "user_email" "text", "user_full_name" "text", "company_name" "text", "p_terms_accepted" boolean) TO "anon";
GRANT ALL ON FUNCTION "public"."create_company_and_user"("user_id" "uuid", "user_email" "text", "user_full_name" "text", "company_name" "text", "p_terms_accepted" boolean) TO "authenticated";
GRANT ALL ON FUNCTION "public"."create_company_and_user"("user_id" "uuid", "user_email" "text", "user_full_name" "text", "company_name" "text", "p_terms_accepted" boolean) TO "service_role";



GRANT ALL ON FUNCTION "public"."create_invitation"("p_email" "text", "p_role" "public"."user_role") TO "anon";
GRANT ALL ON FUNCTION "public"."create_invitation"("p_email" "text", "p_role" "public"."user_role") TO "authenticated";
GRANT ALL ON FUNCTION "public"."create_invitation"("p_email" "text", "p_role" "public"."user_role") TO "service_role";



GRANT ALL ON FUNCTION "public"."create_trial_subscription_for_company"() TO "anon";
GRANT ALL ON FUNCTION "public"."create_trial_subscription_for_company"() TO "authenticated";
GRANT ALL ON FUNCTION "public"."create_trial_subscription_for_company"() TO "service_role";



GRANT ALL ON FUNCTION "public"."get_available_slots"("p_company_slug" "text", "p_date" "date") TO "anon";
GRANT ALL ON FUNCTION "public"."get_available_slots"("p_company_slug" "text", "p_date" "date") TO "authenticated";
GRANT ALL ON FUNCTION "public"."get_available_slots"("p_company_slug" "text", "p_date" "date") TO "service_role";



GRANT ALL ON FUNCTION "public"."get_client_inspector_ids"() TO "anon";
GRANT ALL ON FUNCTION "public"."get_client_inspector_ids"() TO "authenticated";
GRANT ALL ON FUNCTION "public"."get_client_inspector_ids"() TO "service_role";



GRANT ALL ON FUNCTION "public"."get_client_invitation_details"("p_token" "text") TO "anon";
GRANT ALL ON FUNCTION "public"."get_client_invitation_details"("p_token" "text") TO "authenticated";
GRANT ALL ON FUNCTION "public"."get_client_invitation_details"("p_token" "text") TO "service_role";



GRANT ALL ON FUNCTION "public"."get_company_by_slug"("p_slug" "text") TO "anon";
GRANT ALL ON FUNCTION "public"."get_company_by_slug"("p_slug" "text") TO "authenticated";
GRANT ALL ON FUNCTION "public"."get_company_by_slug"("p_slug" "text") TO "service_role";



GRANT ALL ON FUNCTION "public"."get_company_subscription_status"() TO "anon";
GRANT ALL ON FUNCTION "public"."get_company_subscription_status"() TO "authenticated";
GRANT ALL ON FUNCTION "public"."get_company_subscription_status"() TO "service_role";



GRANT ALL ON FUNCTION "public"."get_invitation_by_token"("p_token" "text") TO "anon";
GRANT ALL ON FUNCTION "public"."get_invitation_by_token"("p_token" "text") TO "authenticated";
GRANT ALL ON FUNCTION "public"."get_invitation_by_token"("p_token" "text") TO "service_role";



GRANT ALL ON FUNCTION "public"."get_platform_customer_clients"("p_company_id" "uuid") TO "anon";
GRANT ALL ON FUNCTION "public"."get_platform_customer_clients"("p_company_id" "uuid") TO "authenticated";
GRANT ALL ON FUNCTION "public"."get_platform_customer_clients"("p_company_id" "uuid") TO "service_role";



GRANT ALL ON FUNCTION "public"."get_platform_customer_detail"("p_company_id" "uuid") TO "anon";
GRANT ALL ON FUNCTION "public"."get_platform_customer_detail"("p_company_id" "uuid") TO "authenticated";
GRANT ALL ON FUNCTION "public"."get_platform_customer_detail"("p_company_id" "uuid") TO "service_role";



GRANT ALL ON FUNCTION "public"."get_platform_customer_inspections"("p_company_id" "uuid") TO "anon";
GRANT ALL ON FUNCTION "public"."get_platform_customer_inspections"("p_company_id" "uuid") TO "authenticated";
GRANT ALL ON FUNCTION "public"."get_platform_customer_inspections"("p_company_id" "uuid") TO "service_role";



GRANT ALL ON FUNCTION "public"."get_platform_customer_invitations"("p_company_id" "uuid") TO "anon";
GRANT ALL ON FUNCTION "public"."get_platform_customer_invitations"("p_company_id" "uuid") TO "authenticated";
GRANT ALL ON FUNCTION "public"."get_platform_customer_invitations"("p_company_id" "uuid") TO "service_role";



GRANT ALL ON FUNCTION "public"."get_platform_customer_invoices"("p_company_id" "uuid") TO "anon";
GRANT ALL ON FUNCTION "public"."get_platform_customer_invoices"("p_company_id" "uuid") TO "authenticated";
GRANT ALL ON FUNCTION "public"."get_platform_customer_invoices"("p_company_id" "uuid") TO "service_role";



GRANT ALL ON FUNCTION "public"."get_platform_customer_members"("p_company_id" "uuid") TO "anon";
GRANT ALL ON FUNCTION "public"."get_platform_customer_members"("p_company_id" "uuid") TO "authenticated";
GRANT ALL ON FUNCTION "public"."get_platform_customer_members"("p_company_id" "uuid") TO "service_role";



GRANT ALL ON FUNCTION "public"."get_platform_customer_templates"("p_company_id" "uuid") TO "anon";
GRANT ALL ON FUNCTION "public"."get_platform_customer_templates"("p_company_id" "uuid") TO "authenticated";
GRANT ALL ON FUNCTION "public"."get_platform_customer_templates"("p_company_id" "uuid") TO "service_role";



GRANT ALL ON FUNCTION "public"."get_platform_customers"("p_search_term" "text", "p_page" integer, "p_page_size" integer) TO "anon";
GRANT ALL ON FUNCTION "public"."get_platform_customers"("p_search_term" "text", "p_page" integer, "p_page_size" integer) TO "authenticated";
GRANT ALL ON FUNCTION "public"."get_platform_customers"("p_search_term" "text", "p_page" integer, "p_page_size" integer) TO "service_role";



GRANT ALL ON FUNCTION "public"."get_platform_staff"() TO "anon";
GRANT ALL ON FUNCTION "public"."get_platform_staff"() TO "authenticated";
GRANT ALL ON FUNCTION "public"."get_platform_staff"() TO "service_role";



GRANT ALL ON FUNCTION "public"."get_user_companies"() TO "anon";
GRANT ALL ON FUNCTION "public"."get_user_companies"() TO "authenticated";
GRANT ALL ON FUNCTION "public"."get_user_companies"() TO "service_role";



GRANT ALL ON FUNCTION "public"."get_user_company_id"() TO "anon";
GRANT ALL ON FUNCTION "public"."get_user_company_id"() TO "authenticated";
GRANT ALL ON FUNCTION "public"."get_user_company_id"() TO "service_role";



GRANT ALL ON FUNCTION "public"."get_user_role"() TO "anon";
GRANT ALL ON FUNCTION "public"."get_user_role"() TO "authenticated";
GRANT ALL ON FUNCTION "public"."get_user_role"() TO "service_role";



GRANT ALL ON FUNCTION "public"."has_platform_permission"("required_permission" "public"."platform_permission") TO "anon";
GRANT ALL ON FUNCTION "public"."has_platform_permission"("required_permission" "public"."platform_permission") TO "authenticated";
GRANT ALL ON FUNCTION "public"."has_platform_permission"("required_permission" "public"."platform_permission") TO "service_role";



GRANT ALL ON FUNCTION "public"."insert_default_availability"("p_user_id" "uuid", "p_company_id" "uuid") TO "anon";
GRANT ALL ON FUNCTION "public"."insert_default_availability"("p_user_id" "uuid", "p_company_id" "uuid") TO "authenticated";
GRANT ALL ON FUNCTION "public"."insert_default_availability"("p_user_id" "uuid", "p_company_id" "uuid") TO "service_role";



GRANT ALL ON FUNCTION "public"."is_admin"() TO "anon";
GRANT ALL ON FUNCTION "public"."is_admin"() TO "authenticated";
GRANT ALL ON FUNCTION "public"."is_admin"() TO "service_role";



GRANT ALL ON FUNCTION "public"."is_platform_staff"() TO "anon";
GRANT ALL ON FUNCTION "public"."is_platform_staff"() TO "authenticated";
GRANT ALL ON FUNCTION "public"."is_platform_staff"() TO "service_role";



GRANT ALL ON FUNCTION "public"."is_staff"() TO "anon";
GRANT ALL ON FUNCTION "public"."is_staff"() TO "authenticated";
GRANT ALL ON FUNCTION "public"."is_staff"() TO "service_role";



GRANT ALL ON FUNCTION "public"."mark_expired_invitations"() TO "anon";
GRANT ALL ON FUNCTION "public"."mark_expired_invitations"() TO "authenticated";
GRANT ALL ON FUNCTION "public"."mark_expired_invitations"() TO "service_role";



GRANT ALL ON FUNCTION "public"."resend_invitation"("p_invitation_id" "uuid") TO "anon";
GRANT ALL ON FUNCTION "public"."resend_invitation"("p_invitation_id" "uuid") TO "authenticated";
GRANT ALL ON FUNCTION "public"."resend_invitation"("p_invitation_id" "uuid") TO "service_role";



GRANT ALL ON FUNCTION "public"."revoke_invitation"("p_invitation_id" "uuid") TO "anon";
GRANT ALL ON FUNCTION "public"."revoke_invitation"("p_invitation_id" "uuid") TO "authenticated";
GRANT ALL ON FUNCTION "public"."revoke_invitation"("p_invitation_id" "uuid") TO "service_role";



GRANT ALL ON FUNCTION "public"."switch_company"("p_company_id" "uuid") TO "anon";
GRANT ALL ON FUNCTION "public"."switch_company"("p_company_id" "uuid") TO "authenticated";
GRANT ALL ON FUNCTION "public"."switch_company"("p_company_id" "uuid") TO "service_role";



GRANT ALL ON FUNCTION "public"."temp_read_storage_file"("bucket" "text", "file_path" "text") TO "anon";
GRANT ALL ON FUNCTION "public"."temp_read_storage_file"("bucket" "text", "file_path" "text") TO "authenticated";
GRANT ALL ON FUNCTION "public"."temp_read_storage_file"("bucket" "text", "file_path" "text") TO "service_role";



GRANT ALL ON FUNCTION "public"."update_feedback_updated_at"() TO "anon";
GRANT ALL ON FUNCTION "public"."update_feedback_updated_at"() TO "authenticated";
GRANT ALL ON FUNCTION "public"."update_feedback_updated_at"() TO "service_role";



GRANT ALL ON FUNCTION "public"."update_sample_template_counts"() TO "anon";
GRANT ALL ON FUNCTION "public"."update_sample_template_counts"() TO "authenticated";
GRANT ALL ON FUNCTION "public"."update_sample_template_counts"() TO "service_role";



GRANT ALL ON FUNCTION "public"."update_updated_at"() TO "anon";
GRANT ALL ON FUNCTION "public"."update_updated_at"() TO "authenticated";
GRANT ALL ON FUNCTION "public"."update_updated_at"() TO "service_role";



GRANT ALL ON TABLE "public"."availability_schedules" TO "anon";
GRANT ALL ON TABLE "public"."availability_schedules" TO "authenticated";
GRANT ALL ON TABLE "public"."availability_schedules" TO "service_role";



GRANT ALL ON TABLE "public"."client_invitations" TO "anon";
GRANT ALL ON TABLE "public"."client_invitations" TO "authenticated";
GRANT ALL ON TABLE "public"."client_invitations" TO "service_role";



GRANT ALL ON TABLE "public"."clients" TO "anon";
GRANT ALL ON TABLE "public"."clients" TO "authenticated";
GRANT ALL ON TABLE "public"."clients" TO "service_role";



GRANT ALL ON TABLE "public"."companies" TO "anon";
GRANT ALL ON TABLE "public"."companies" TO "authenticated";
GRANT ALL ON TABLE "public"."companies" TO "service_role";



GRANT ALL ON TABLE "public"."company_memberships" TO "anon";
GRANT ALL ON TABLE "public"."company_memberships" TO "authenticated";
GRANT ALL ON TABLE "public"."company_memberships" TO "service_role";



GRANT ALL ON TABLE "public"."company_subscriptions" TO "anon";
GRANT ALL ON TABLE "public"."company_subscriptions" TO "authenticated";
GRANT ALL ON TABLE "public"."company_subscriptions" TO "service_role";



GRANT ALL ON TABLE "public"."feedback" TO "anon";
GRANT ALL ON TABLE "public"."feedback" TO "authenticated";
GRANT ALL ON TABLE "public"."feedback" TO "service_role";



GRANT ALL ON TABLE "public"."feedback_attachments" TO "anon";
GRANT ALL ON TABLE "public"."feedback_attachments" TO "authenticated";
GRANT ALL ON TABLE "public"."feedback_attachments" TO "service_role";



GRANT ALL ON TABLE "public"."google_calendar_connections" TO "anon";
GRANT ALL ON TABLE "public"."google_calendar_connections" TO "authenticated";
GRANT ALL ON TABLE "public"."google_calendar_connections" TO "service_role";



GRANT ALL ON TABLE "public"."inspection_email_log" TO "anon";
GRANT ALL ON TABLE "public"."inspection_email_log" TO "authenticated";
GRANT ALL ON TABLE "public"."inspection_email_log" TO "service_role";



GRANT ALL ON TABLE "public"."inspection_templates" TO "anon";
GRANT ALL ON TABLE "public"."inspection_templates" TO "authenticated";
GRANT ALL ON TABLE "public"."inspection_templates" TO "service_role";



GRANT ALL ON TABLE "public"."inspections" TO "anon";
GRANT ALL ON TABLE "public"."inspections" TO "authenticated";
GRANT ALL ON TABLE "public"."inspections" TO "service_role";



GRANT ALL ON TABLE "public"."invitations" TO "anon";
GRANT ALL ON TABLE "public"."invitations" TO "authenticated";
GRANT ALL ON TABLE "public"."invitations" TO "service_role";



GRANT ALL ON TABLE "public"."llm_usage_logs" TO "anon";
GRANT ALL ON TABLE "public"."llm_usage_logs" TO "authenticated";
GRANT ALL ON TABLE "public"."llm_usage_logs" TO "service_role";



GRANT ALL ON TABLE "public"."platform_coupons" TO "anon";
GRANT ALL ON TABLE "public"."platform_coupons" TO "authenticated";
GRANT ALL ON TABLE "public"."platform_coupons" TO "service_role";



GRANT ALL ON TABLE "public"."platform_promotion_codes" TO "anon";
GRANT ALL ON TABLE "public"."platform_promotion_codes" TO "authenticated";
GRANT ALL ON TABLE "public"."platform_promotion_codes" TO "service_role";



GRANT ALL ON TABLE "public"."platform_staff" TO "anon";
GRANT ALL ON TABLE "public"."platform_staff" TO "authenticated";
GRANT ALL ON TABLE "public"."platform_staff" TO "service_role";



GRANT ALL ON TABLE "public"."platform_staff_permissions" TO "anon";
GRANT ALL ON TABLE "public"."platform_staff_permissions" TO "authenticated";
GRANT ALL ON TABLE "public"."platform_staff_permissions" TO "service_role";



GRANT ALL ON TABLE "public"."report_versions" TO "anon";
GRANT ALL ON TABLE "public"."report_versions" TO "authenticated";
GRANT ALL ON TABLE "public"."report_versions" TO "service_role";



GRANT ALL ON TABLE "public"."reports" TO "anon";
GRANT ALL ON TABLE "public"."reports" TO "authenticated";
GRANT ALL ON TABLE "public"."reports" TO "service_role";



GRANT ALL ON TABLE "public"."sample_templates" TO "anon";
GRANT ALL ON TABLE "public"."sample_templates" TO "authenticated";
GRANT ALL ON TABLE "public"."sample_templates" TO "service_role";



GRANT ALL ON TABLE "public"."subscription_invoices" TO "anon";
GRANT ALL ON TABLE "public"."subscription_invoices" TO "authenticated";
GRANT ALL ON TABLE "public"."subscription_invoices" TO "service_role";



GRANT ALL ON TABLE "public"."users" TO "anon";
GRANT ALL ON TABLE "public"."users" TO "authenticated";
GRANT ALL ON TABLE "public"."users" TO "service_role";



ALTER DEFAULT PRIVILEGES FOR ROLE "postgres" IN SCHEMA "public" GRANT ALL ON SEQUENCES TO "postgres";
ALTER DEFAULT PRIVILEGES FOR ROLE "postgres" IN SCHEMA "public" GRANT ALL ON SEQUENCES TO "anon";
ALTER DEFAULT PRIVILEGES FOR ROLE "postgres" IN SCHEMA "public" GRANT ALL ON SEQUENCES TO "authenticated";
ALTER DEFAULT PRIVILEGES FOR ROLE "postgres" IN SCHEMA "public" GRANT ALL ON SEQUENCES TO "service_role";






ALTER DEFAULT PRIVILEGES FOR ROLE "postgres" IN SCHEMA "public" GRANT ALL ON FUNCTIONS TO "postgres";
ALTER DEFAULT PRIVILEGES FOR ROLE "postgres" IN SCHEMA "public" GRANT ALL ON FUNCTIONS TO "anon";
ALTER DEFAULT PRIVILEGES FOR ROLE "postgres" IN SCHEMA "public" GRANT ALL ON FUNCTIONS TO "authenticated";
ALTER DEFAULT PRIVILEGES FOR ROLE "postgres" IN SCHEMA "public" GRANT ALL ON FUNCTIONS TO "service_role";






ALTER DEFAULT PRIVILEGES FOR ROLE "postgres" IN SCHEMA "public" GRANT ALL ON TABLES TO "postgres";
ALTER DEFAULT PRIVILEGES FOR ROLE "postgres" IN SCHEMA "public" GRANT ALL ON TABLES TO "anon";
ALTER DEFAULT PRIVILEGES FOR ROLE "postgres" IN SCHEMA "public" GRANT ALL ON TABLES TO "authenticated";
ALTER DEFAULT PRIVILEGES FOR ROLE "postgres" IN SCHEMA "public" GRANT ALL ON TABLES TO "service_role";







