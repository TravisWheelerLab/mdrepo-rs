-- Test-database schema fixture for the mdr-db integration tests.
--
-- This is a schema-only (no data) snapshot of the Django-owned mdrepo schema.
-- The schema is owned by the Django models in /opt/mdrepo/django, NOT by diesel,
-- so this file is a point-in-time snapshot and will drift when Django migrations
-- change the schema. Regenerate it (from a machine with staging access) with:
--
--   set -a; . /opt/mdrepo/simulation-processing/python/.env; set +a
--   PGHOST="$PG_STAGING_HOST" PGPORT="$PG_STAGING_PORT" PGUSER="$PG_STAGING_USER" \
--   PGPASSWORD="$PG_STAGING_PASSWORD" PGDATABASE="$PG_STAGING_DBNAME" \
--   pg_dump --schema-only --no-owner --no-privileges \
--     > mdr-db/tests/fixtures/schema.sql
--   # then re-add this header block.
--
--
-- PostgreSQL database dump
--

\restrict t8Epape6afZJHaLtSO2kmPoVZuzZobOU4FXTGNaP49CD87mUjqcz0Darflt6fM8

-- Dumped from database version 14.23 (Ubuntu 14.23-0ubuntu0.22.04.1)
-- Dumped by pg_dump version 16.14 (Ubuntu 16.14-0ubuntu0.24.04.1)

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

--
-- Name: public; Type: SCHEMA; Schema: -; Owner: -
--

-- *not* creating schema, since initdb creates it


--
-- Name: btree_gin; Type: EXTENSION; Schema: -; Owner: -
--

CREATE EXTENSION IF NOT EXISTS btree_gin WITH SCHEMA public;


--
-- Name: EXTENSION btree_gin; Type: COMMENT; Schema: -; Owner: -
--

COMMENT ON EXTENSION btree_gin IS 'support for indexing common datatypes in GIN';


--
-- Name: pg_trgm; Type: EXTENSION; Schema: -; Owner: -
--

CREATE EXTENSION IF NOT EXISTS pg_trgm WITH SCHEMA public;


--
-- Name: EXTENSION pg_trgm; Type: COMMENT; Schema: -; Owner: -
--

COMMENT ON EXTENSION pg_trgm IS 'text similarity measurement and index searching based on trigrams';


--
-- Name: _pgh_attach_context(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public._pgh_attach_context() RETURNS uuid
    LANGUAGE plpgsql
    AS $$
                    DECLARE
                        _pgh_context_id UUID;
                        _pgh_context_metadata JSONB;
                    BEGIN
                        BEGIN
                            SELECT INTO _pgh_context_id
                                CURRENT_SETTING('pghistory.context_id');
                            SELECT INTO _pgh_context_metadata
                                CURRENT_SETTING('pghistory.context_metadata');
                            EXCEPTION WHEN OTHERS THEN
                        END;
                        IF _pgh_context_id IS NOT NULL AND _pgh_context_metadata IS NOT NULL THEN
                            INSERT INTO pghistory_context (id, metadata, created_at, updated_at)
                                VALUES (_pgh_context_id, _pgh_context_metadata, NOW(), NOW())
                                ON CONFLICT (id) DO UPDATE
                                    SET metadata = EXCLUDED.metadata,
                                        updated_at = EXCLUDED.updated_at
                                    WHERE pghistory_context.metadata != EXCLUDED.metadata;
                            RETURN _pgh_context_id;
                        ELSE
                            RETURN NULL;
                        END IF;
                    END;
                $$;


--
-- Name: _pgtrigger_should_ignore(name); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public._pgtrigger_should_ignore(trigger_name name) RETURNS boolean
    LANGUAGE plpgsql
    AS $$
                DECLARE
                    _pgtrigger_ignore TEXT[];
                    _result BOOLEAN;
                BEGIN
                    BEGIN
                        SELECT INTO _pgtrigger_ignore
                            CURRENT_SETTING('pgtrigger.ignore');
                        EXCEPTION WHEN OTHERS THEN
                    END;
                    IF _pgtrigger_ignore IS NOT NULL THEN
                        SELECT trigger_name = ANY(_pgtrigger_ignore)
                        INTO _result;
                        RETURN _result;
                    ELSE
                        RETURN FALSE;
                    END IF;
                END;
            $$;


--
-- Name: pgtrigger_delete_delete_38b08(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.pgtrigger_delete_delete_38b08() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
                
                BEGIN
                    IF ("public"._pgtrigger_should_ignore(TG_NAME) IS TRUE) THEN
                        IF (TG_OP = 'DELETE') THEN
                            RETURN OLD;
                        ELSE
                            RETURN NEW;
                        END IF;
                    END IF;
                    INSERT INTO "md_repo_app_simulationevent" ("created_by_id", "creation_date", "description", "display_trajectory_file_n_frames", "duration", "external_link", "fasta_sequence", "forcefield", "forcefield_comments", "guid", "id", "includes_water", "integration_timestep_fs", "is_deprecated", "is_placeholder", "is_restricted", "md_repo_ticket_id", "pdb_id", "pgh_context_id", "pgh_created_at", "pgh_label", "pgh_obj_id", "protonation_method", "replicate", "replicate_group_id", "rmsd_values", "rmsf_values", "run_commands", "sampling_frequency", "short_description", "software_id", "temperature", "three_letter_amino_acid_sequence", "total_replicates", "unique_file_hash_string", "water_density", "water_density_units", "water_type") VALUES (OLD."created_by_id", OLD."creation_date", OLD."description", OLD."display_trajectory_file_n_frames", OLD."duration", OLD."external_link", OLD."fasta_sequence", OLD."forcefield", OLD."forcefield_comments", OLD."guid", OLD."id", OLD."includes_water", OLD."integration_timestep_fs", OLD."is_deprecated", OLD."is_placeholder", OLD."is_restricted", OLD."md_repo_ticket_id", OLD."pdb_id", _pgh_attach_context(), NOW(), 'delete', OLD."id", OLD."protonation_method", OLD."replicate", OLD."replicate_group_id", OLD."rmsd_values", OLD."rmsf_values", OLD."run_commands", OLD."sampling_frequency", OLD."short_description", OLD."software_id", OLD."temperature", OLD."three_letter_amino_acid_sequence", OLD."total_replicates", OLD."unique_file_hash_string", OLD."water_density", OLD."water_density_units", OLD."water_type"); RETURN NULL;
                END;
            $$;


--
-- Name: pgtrigger_delete_delete_66c1c(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.pgtrigger_delete_delete_66c1c() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
                
                BEGIN
                    IF ("public"._pgtrigger_should_ignore(TG_NAME) IS TRUE) THEN
                        IF (TG_OP = 'DELETE') THEN
                            RETURN OLD;
                        ELSE
                            RETURN NEW;
                        END IF;
                    END IF;
                    INSERT INTO "md_repo_app_solventevent" ("concentration", "concentration_units", "id", "name", "pgh_context_id", "pgh_created_at", "pgh_label", "pgh_obj_id", "simulation_id") VALUES (OLD."concentration", OLD."concentration_units", OLD."id", OLD."name", _pgh_attach_context(), NOW(), 'delete', OLD."id", OLD."simulation_id"); RETURN NULL;
                END;
            $$;


--
-- Name: pgtrigger_delete_delete_69b83(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.pgtrigger_delete_delete_69b83() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
                
                BEGIN
                    IF ("public"._pgtrigger_should_ignore(TG_NAME) IS TRUE) THEN
                        IF (TG_OP = 'DELETE') THEN
                            RETURN OLD;
                        ELSE
                            RETURN NEW;
                        END IF;
                    END IF;
                    INSERT INTO "md_repo_app_ligandevent" ("id", "name", "pgh_context_id", "pgh_created_at", "pgh_label", "pgh_obj_id", "simulation_id", "smiles_string") VALUES (OLD."id", OLD."name", _pgh_attach_context(), NOW(), 'delete', OLD."id", OLD."simulation_id", OLD."smiles_string"); RETURN NULL;
                END;
            $$;


--
-- Name: pgtrigger_delete_delete_6c42f(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.pgtrigger_delete_delete_6c42f() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
                
                BEGIN
                    IF ("public"._pgtrigger_should_ignore(TG_NAME) IS TRUE) THEN
                        IF (TG_OP = 'DELETE') THEN
                            RETURN OLD;
                        ELSE
                            RETURN NEW;
                        END IF;
                    END IF;
                    INSERT INTO "md_repo_app_contributionevent" ("email", "id", "institution", "name", "orcid", "pgh_context_id", "pgh_created_at", "pgh_label", "pgh_obj_id", "rank", "simulation_id") VALUES (OLD."email", OLD."id", OLD."institution", OLD."name", OLD."orcid", _pgh_attach_context(), NOW(), 'delete', OLD."id", OLD."rank", OLD."simulation_id"); RETURN NULL;
                END;
            $$;


--
-- Name: pgtrigger_delete_delete_83e3e(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.pgtrigger_delete_delete_83e3e() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
                
                BEGIN
                    IF ("public"._pgtrigger_should_ignore(TG_NAME) IS TRUE) THEN
                        IF (TG_OP = 'DELETE') THEN
                            RETURN OLD;
                        ELSE
                            RETURN NEW;
                        END IF;
                    END IF;
                    INSERT INTO "md_repo_app_simulationsoftwareevent" ("id", "name", "pgh_context_id", "pgh_created_at", "pgh_label", "pgh_obj_id", "version") VALUES (OLD."id", OLD."name", _pgh_attach_context(), NOW(), 'delete', OLD."id", OLD."version"); RETURN NULL;
                END;
            $$;


--
-- Name: pgtrigger_delete_delete_af16b(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.pgtrigger_delete_delete_af16b() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
                
                BEGIN
                    IF ("public"._pgtrigger_should_ignore(TG_NAME) IS TRUE) THEN
                        IF (TG_OP = 'DELETE') THEN
                            RETURN OLD;
                        ELSE
                            RETURN NEW;
                        END IF;
                    END IF;
                    INSERT INTO "md_repo_app_biomoleculeevent" ("amino_length", "id", "name", "pdb_id", "pgh_context_id", "pgh_created_at", "pgh_label", "pgh_obj_id", "primary_molecule_id_type", "sequence", "uniprot_id") VALUES (OLD."amino_length", OLD."id", OLD."name", OLD."pdb_id", _pgh_attach_context(), NOW(), 'delete', OLD."id", OLD."primary_molecule_id_type", OLD."sequence", OLD."uniprot_id"); RETURN NULL;
                END;
            $$;


--
-- Name: pgtrigger_delete_delete_d446d(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.pgtrigger_delete_delete_d446d() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
                
                BEGIN
                    IF ("public"._pgtrigger_should_ignore(TG_NAME) IS TRUE) THEN
                        IF (TG_OP = 'DELETE') THEN
                            RETURN OLD;
                        ELSE
                            RETURN NEW;
                        END IF;
                    END IF;
                    INSERT INTO "md_repo_app_paperevent" ("authors", "doi", "id", "journal", "number", "pages", "pgh_context_id", "pgh_created_at", "pgh_label", "pgh_obj_id", "simulation_id", "title", "volume", "year") VALUES (OLD."authors", OLD."doi", OLD."id", OLD."journal", OLD."number", OLD."pages", _pgh_attach_context(), NOW(), 'delete', OLD."id", OLD."simulation_id", OLD."title", OLD."volume", OLD."year"); RETURN NULL;
                END;
            $$;


--
-- Name: pgtrigger_delete_delete_d8061(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.pgtrigger_delete_delete_d8061() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
                
                BEGIN
                    IF ("public"._pgtrigger_should_ignore(TG_NAME) IS TRUE) THEN
                        IF (TG_OP = 'DELETE') THEN
                            RETURN OLD;
                        ELSE
                            RETURN NEW;
                        END IF;
                    END IF;
                    INSERT INTO "md_repo_app_linkedbiomoleculeevent" ("biomolecule_id_id", "id", "pgh_context_id", "pgh_created_at", "pgh_label", "pgh_obj_id") VALUES (OLD."biomolecule_id_id", OLD."id", _pgh_attach_context(), NOW(), 'delete', OLD."id"); RETURN NULL;
                END;
            $$;


--
-- Name: pgtrigger_delete_delete_fdc3a(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.pgtrigger_delete_delete_fdc3a() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
                
                BEGIN
                    IF ("public"._pgtrigger_should_ignore(TG_NAME) IS TRUE) THEN
                        IF (TG_OP = 'DELETE') THEN
                            RETURN OLD;
                        ELSE
                            RETURN NEW;
                        END IF;
                    END IF;
                    INSERT INTO "md_repo_app_unvalidatedbiomoleculeevent" ("id", "molecule_id", "molecule_id_type", "pgh_context_id", "pgh_created_at", "pgh_label", "pgh_obj_id", "simulation_id") VALUES (OLD."id", OLD."molecule_id", OLD."molecule_id_type", _pgh_attach_context(), NOW(), 'delete', OLD."id", OLD."simulation_id"); RETURN NULL;
                END;
            $$;


--
-- Name: pgtrigger_insert_insert_07bd1(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.pgtrigger_insert_insert_07bd1() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
                
                BEGIN
                    IF ("public"._pgtrigger_should_ignore(TG_NAME) IS TRUE) THEN
                        IF (TG_OP = 'DELETE') THEN
                            RETURN OLD;
                        ELSE
                            RETURN NEW;
                        END IF;
                    END IF;
                    INSERT INTO "md_repo_app_unvalidatedbiomoleculeevent" ("id", "molecule_id", "molecule_id_type", "pgh_context_id", "pgh_created_at", "pgh_label", "pgh_obj_id", "simulation_id") VALUES (NEW."id", NEW."molecule_id", NEW."molecule_id_type", _pgh_attach_context(), NOW(), 'insert', NEW."id", NEW."simulation_id"); RETURN NULL;
                END;
            $$;


--
-- Name: pgtrigger_insert_insert_21c15(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.pgtrigger_insert_insert_21c15() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
                
                BEGIN
                    IF ("public"._pgtrigger_should_ignore(TG_NAME) IS TRUE) THEN
                        IF (TG_OP = 'DELETE') THEN
                            RETURN OLD;
                        ELSE
                            RETURN NEW;
                        END IF;
                    END IF;
                    INSERT INTO "md_repo_app_contributionevent" ("email", "id", "institution", "name", "orcid", "pgh_context_id", "pgh_created_at", "pgh_label", "pgh_obj_id", "rank", "simulation_id") VALUES (NEW."email", NEW."id", NEW."institution", NEW."name", NEW."orcid", _pgh_attach_context(), NOW(), 'insert', NEW."id", NEW."rank", NEW."simulation_id"); RETURN NULL;
                END;
            $$;


--
-- Name: pgtrigger_insert_insert_2bcb1(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.pgtrigger_insert_insert_2bcb1() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
                
                BEGIN
                    IF ("public"._pgtrigger_should_ignore(TG_NAME) IS TRUE) THEN
                        IF (TG_OP = 'DELETE') THEN
                            RETURN OLD;
                        ELSE
                            RETURN NEW;
                        END IF;
                    END IF;
                    INSERT INTO "md_repo_app_paperevent" ("authors", "doi", "id", "journal", "number", "pages", "pgh_context_id", "pgh_created_at", "pgh_label", "pgh_obj_id", "simulation_id", "title", "volume", "year") VALUES (NEW."authors", NEW."doi", NEW."id", NEW."journal", NEW."number", NEW."pages", _pgh_attach_context(), NOW(), 'insert', NEW."id", NEW."simulation_id", NEW."title", NEW."volume", NEW."year"); RETURN NULL;
                END;
            $$;


--
-- Name: pgtrigger_insert_insert_4385d(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.pgtrigger_insert_insert_4385d() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
                
                BEGIN
                    IF ("public"._pgtrigger_should_ignore(TG_NAME) IS TRUE) THEN
                        IF (TG_OP = 'DELETE') THEN
                            RETURN OLD;
                        ELSE
                            RETURN NEW;
                        END IF;
                    END IF;
                    INSERT INTO "md_repo_app_solventevent" ("concentration", "concentration_units", "id", "name", "pgh_context_id", "pgh_created_at", "pgh_label", "pgh_obj_id", "simulation_id") VALUES (NEW."concentration", NEW."concentration_units", NEW."id", NEW."name", _pgh_attach_context(), NOW(), 'insert', NEW."id", NEW."simulation_id"); RETURN NULL;
                END;
            $$;


--
-- Name: pgtrigger_insert_insert_73504(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.pgtrigger_insert_insert_73504() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
                
                BEGIN
                    IF ("public"._pgtrigger_should_ignore(TG_NAME) IS TRUE) THEN
                        IF (TG_OP = 'DELETE') THEN
                            RETURN OLD;
                        ELSE
                            RETURN NEW;
                        END IF;
                    END IF;
                    INSERT INTO "md_repo_app_biomoleculeevent" ("amino_length", "id", "name", "pdb_id", "pgh_context_id", "pgh_created_at", "pgh_label", "pgh_obj_id", "primary_molecule_id_type", "sequence", "uniprot_id") VALUES (NEW."amino_length", NEW."id", NEW."name", NEW."pdb_id", _pgh_attach_context(), NOW(), 'insert', NEW."id", NEW."primary_molecule_id_type", NEW."sequence", NEW."uniprot_id"); RETURN NULL;
                END;
            $$;


--
-- Name: pgtrigger_insert_insert_7b0ae(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.pgtrigger_insert_insert_7b0ae() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
                
                BEGIN
                    IF ("public"._pgtrigger_should_ignore(TG_NAME) IS TRUE) THEN
                        IF (TG_OP = 'DELETE') THEN
                            RETURN OLD;
                        ELSE
                            RETURN NEW;
                        END IF;
                    END IF;
                    INSERT INTO "md_repo_app_linkedbiomoleculeevent" ("biomolecule_id_id", "id", "pgh_context_id", "pgh_created_at", "pgh_label", "pgh_obj_id") VALUES (NEW."biomolecule_id_id", NEW."id", _pgh_attach_context(), NOW(), 'insert', NEW."id"); RETURN NULL;
                END;
            $$;


--
-- Name: pgtrigger_insert_insert_92791(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.pgtrigger_insert_insert_92791() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
                
                BEGIN
                    IF ("public"._pgtrigger_should_ignore(TG_NAME) IS TRUE) THEN
                        IF (TG_OP = 'DELETE') THEN
                            RETURN OLD;
                        ELSE
                            RETURN NEW;
                        END IF;
                    END IF;
                    INSERT INTO "md_repo_app_simulationsoftwareevent" ("id", "name", "pgh_context_id", "pgh_created_at", "pgh_label", "pgh_obj_id", "version") VALUES (NEW."id", NEW."name", _pgh_attach_context(), NOW(), 'insert', NEW."id", NEW."version"); RETURN NULL;
                END;
            $$;


--
-- Name: pgtrigger_insert_insert_c661e(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.pgtrigger_insert_insert_c661e() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
                
                BEGIN
                    IF ("public"._pgtrigger_should_ignore(TG_NAME) IS TRUE) THEN
                        IF (TG_OP = 'DELETE') THEN
                            RETURN OLD;
                        ELSE
                            RETURN NEW;
                        END IF;
                    END IF;
                    INSERT INTO "md_repo_app_ligandevent" ("id", "name", "pgh_context_id", "pgh_created_at", "pgh_label", "pgh_obj_id", "simulation_id", "smiles_string") VALUES (NEW."id", NEW."name", _pgh_attach_context(), NOW(), 'insert', NEW."id", NEW."simulation_id", NEW."smiles_string"); RETURN NULL;
                END;
            $$;


--
-- Name: pgtrigger_insert_insert_ec13c(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.pgtrigger_insert_insert_ec13c() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
                
                BEGIN
                    IF ("public"._pgtrigger_should_ignore(TG_NAME) IS TRUE) THEN
                        IF (TG_OP = 'DELETE') THEN
                            RETURN OLD;
                        ELSE
                            RETURN NEW;
                        END IF;
                    END IF;
                    INSERT INTO "md_repo_app_simulationevent" ("created_by_id", "creation_date", "description", "display_trajectory_file_n_frames", "duration", "external_link", "fasta_sequence", "forcefield", "forcefield_comments", "guid", "id", "includes_water", "integration_timestep_fs", "is_deprecated", "is_placeholder", "is_restricted", "md_repo_ticket_id", "pdb_id", "pgh_context_id", "pgh_created_at", "pgh_label", "pgh_obj_id", "protonation_method", "replicate", "replicate_group_id", "rmsd_values", "rmsf_values", "run_commands", "sampling_frequency", "short_description", "software_id", "temperature", "three_letter_amino_acid_sequence", "total_replicates", "unique_file_hash_string", "water_density", "water_density_units", "water_type") VALUES (NEW."created_by_id", NEW."creation_date", NEW."description", NEW."display_trajectory_file_n_frames", NEW."duration", NEW."external_link", NEW."fasta_sequence", NEW."forcefield", NEW."forcefield_comments", NEW."guid", NEW."id", NEW."includes_water", NEW."integration_timestep_fs", NEW."is_deprecated", NEW."is_placeholder", NEW."is_restricted", NEW."md_repo_ticket_id", NEW."pdb_id", _pgh_attach_context(), NOW(), 'insert', NEW."id", NEW."protonation_method", NEW."replicate", NEW."replicate_group_id", NEW."rmsd_values", NEW."rmsf_values", NEW."run_commands", NEW."sampling_frequency", NEW."short_description", NEW."software_id", NEW."temperature", NEW."three_letter_amino_acid_sequence", NEW."total_replicates", NEW."unique_file_hash_string", NEW."water_density", NEW."water_density_units", NEW."water_type"); RETURN NULL;
                END;
            $$;


--
-- Name: pgtrigger_update_update_0cda0(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.pgtrigger_update_update_0cda0() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
                
                BEGIN
                    IF ("public"._pgtrigger_should_ignore(TG_NAME) IS TRUE) THEN
                        IF (TG_OP = 'DELETE') THEN
                            RETURN OLD;
                        ELSE
                            RETURN NEW;
                        END IF;
                    END IF;
                    INSERT INTO "md_repo_app_simulationsoftwareevent" ("id", "name", "pgh_context_id", "pgh_created_at", "pgh_label", "pgh_obj_id", "version") VALUES (NEW."id", NEW."name", _pgh_attach_context(), NOW(), 'update', NEW."id", NEW."version"); RETURN NULL;
                END;
            $$;


--
-- Name: pgtrigger_update_update_1112b(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.pgtrigger_update_update_1112b() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
                
                BEGIN
                    IF ("public"._pgtrigger_should_ignore(TG_NAME) IS TRUE) THEN
                        IF (TG_OP = 'DELETE') THEN
                            RETURN OLD;
                        ELSE
                            RETURN NEW;
                        END IF;
                    END IF;
                    INSERT INTO "md_repo_app_ligandevent" ("id", "name", "pgh_context_id", "pgh_created_at", "pgh_label", "pgh_obj_id", "simulation_id", "smiles_string") VALUES (NEW."id", NEW."name", _pgh_attach_context(), NOW(), 'update', NEW."id", NEW."simulation_id", NEW."smiles_string"); RETURN NULL;
                END;
            $$;


--
-- Name: pgtrigger_update_update_47d84(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.pgtrigger_update_update_47d84() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
                
                BEGIN
                    IF ("public"._pgtrigger_should_ignore(TG_NAME) IS TRUE) THEN
                        IF (TG_OP = 'DELETE') THEN
                            RETURN OLD;
                        ELSE
                            RETURN NEW;
                        END IF;
                    END IF;
                    INSERT INTO "md_repo_app_linkedbiomoleculeevent" ("biomolecule_id_id", "id", "pgh_context_id", "pgh_created_at", "pgh_label", "pgh_obj_id") VALUES (NEW."biomolecule_id_id", NEW."id", _pgh_attach_context(), NOW(), 'update', NEW."id"); RETURN NULL;
                END;
            $$;


--
-- Name: pgtrigger_update_update_588a0(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.pgtrigger_update_update_588a0() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
                
                BEGIN
                    IF ("public"._pgtrigger_should_ignore(TG_NAME) IS TRUE) THEN
                        IF (TG_OP = 'DELETE') THEN
                            RETURN OLD;
                        ELSE
                            RETURN NEW;
                        END IF;
                    END IF;
                    INSERT INTO "md_repo_app_biomoleculeevent" ("amino_length", "id", "name", "pdb_id", "pgh_context_id", "pgh_created_at", "pgh_label", "pgh_obj_id", "primary_molecule_id_type", "sequence", "uniprot_id") VALUES (NEW."amino_length", NEW."id", NEW."name", NEW."pdb_id", _pgh_attach_context(), NOW(), 'update', NEW."id", NEW."primary_molecule_id_type", NEW."sequence", NEW."uniprot_id"); RETURN NULL;
                END;
            $$;


--
-- Name: pgtrigger_update_update_5ca93(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.pgtrigger_update_update_5ca93() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
                
                BEGIN
                    IF ("public"._pgtrigger_should_ignore(TG_NAME) IS TRUE) THEN
                        IF (TG_OP = 'DELETE') THEN
                            RETURN OLD;
                        ELSE
                            RETURN NEW;
                        END IF;
                    END IF;
                    INSERT INTO "md_repo_app_unvalidatedbiomoleculeevent" ("id", "molecule_id", "molecule_id_type", "pgh_context_id", "pgh_created_at", "pgh_label", "pgh_obj_id", "simulation_id") VALUES (NEW."id", NEW."molecule_id", NEW."molecule_id_type", _pgh_attach_context(), NOW(), 'update', NEW."id", NEW."simulation_id"); RETURN NULL;
                END;
            $$;


--
-- Name: pgtrigger_update_update_6c8bc(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.pgtrigger_update_update_6c8bc() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
                
                BEGIN
                    IF ("public"._pgtrigger_should_ignore(TG_NAME) IS TRUE) THEN
                        IF (TG_OP = 'DELETE') THEN
                            RETURN OLD;
                        ELSE
                            RETURN NEW;
                        END IF;
                    END IF;
                    INSERT INTO "md_repo_app_paperevent" ("authors", "doi", "id", "journal", "number", "pages", "pgh_context_id", "pgh_created_at", "pgh_label", "pgh_obj_id", "simulation_id", "title", "volume", "year") VALUES (NEW."authors", NEW."doi", NEW."id", NEW."journal", NEW."number", NEW."pages", _pgh_attach_context(), NOW(), 'update', NEW."id", NEW."simulation_id", NEW."title", NEW."volume", NEW."year"); RETURN NULL;
                END;
            $$;


--
-- Name: pgtrigger_update_update_70073(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.pgtrigger_update_update_70073() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
                
                BEGIN
                    IF ("public"._pgtrigger_should_ignore(TG_NAME) IS TRUE) THEN
                        IF (TG_OP = 'DELETE') THEN
                            RETURN OLD;
                        ELSE
                            RETURN NEW;
                        END IF;
                    END IF;
                    INSERT INTO "md_repo_app_simulationevent" ("created_by_id", "creation_date", "description", "display_trajectory_file_n_frames", "duration", "external_link", "fasta_sequence", "forcefield", "forcefield_comments", "guid", "id", "includes_water", "integration_timestep_fs", "is_deprecated", "is_placeholder", "is_restricted", "md_repo_ticket_id", "pdb_id", "pgh_context_id", "pgh_created_at", "pgh_label", "pgh_obj_id", "protonation_method", "replicate", "replicate_group_id", "rmsd_values", "rmsf_values", "run_commands", "sampling_frequency", "short_description", "software_id", "temperature", "three_letter_amino_acid_sequence", "total_replicates", "unique_file_hash_string", "water_density", "water_density_units", "water_type") VALUES (NEW."created_by_id", NEW."creation_date", NEW."description", NEW."display_trajectory_file_n_frames", NEW."duration", NEW."external_link", NEW."fasta_sequence", NEW."forcefield", NEW."forcefield_comments", NEW."guid", NEW."id", NEW."includes_water", NEW."integration_timestep_fs", NEW."is_deprecated", NEW."is_placeholder", NEW."is_restricted", NEW."md_repo_ticket_id", NEW."pdb_id", _pgh_attach_context(), NOW(), 'update', NEW."id", NEW."protonation_method", NEW."replicate", NEW."replicate_group_id", NEW."rmsd_values", NEW."rmsf_values", NEW."run_commands", NEW."sampling_frequency", NEW."short_description", NEW."software_id", NEW."temperature", NEW."three_letter_amino_acid_sequence", NEW."total_replicates", NEW."unique_file_hash_string", NEW."water_density", NEW."water_density_units", NEW."water_type"); RETURN NULL;
                END;
            $$;


--
-- Name: pgtrigger_update_update_81931(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.pgtrigger_update_update_81931() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
                
                BEGIN
                    IF ("public"._pgtrigger_should_ignore(TG_NAME) IS TRUE) THEN
                        IF (TG_OP = 'DELETE') THEN
                            RETURN OLD;
                        ELSE
                            RETURN NEW;
                        END IF;
                    END IF;
                    INSERT INTO "md_repo_app_solventevent" ("concentration", "concentration_units", "id", "name", "pgh_context_id", "pgh_created_at", "pgh_label", "pgh_obj_id", "simulation_id") VALUES (NEW."concentration", NEW."concentration_units", NEW."id", NEW."name", _pgh_attach_context(), NOW(), 'update', NEW."id", NEW."simulation_id"); RETURN NULL;
                END;
            $$;


--
-- Name: pgtrigger_update_update_8a833(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.pgtrigger_update_update_8a833() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
                
                BEGIN
                    IF ("public"._pgtrigger_should_ignore(TG_NAME) IS TRUE) THEN
                        IF (TG_OP = 'DELETE') THEN
                            RETURN OLD;
                        ELSE
                            RETURN NEW;
                        END IF;
                    END IF;
                    INSERT INTO "md_repo_app_contributionevent" ("email", "id", "institution", "name", "orcid", "pgh_context_id", "pgh_created_at", "pgh_label", "pgh_obj_id", "rank", "simulation_id") VALUES (NEW."email", NEW."id", NEW."institution", NEW."name", NEW."orcid", _pgh_attach_context(), NOW(), 'update', NEW."id", NEW."rank", NEW."simulation_id"); RETURN NULL;
                END;
            $$;


SET default_tablespace = '';

SET default_table_access_method = heap;

--
-- Name: account_emailaddress; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.account_emailaddress (
    id integer NOT NULL,
    email character varying(254) NOT NULL,
    verified boolean NOT NULL,
    "primary" boolean NOT NULL,
    user_id bigint NOT NULL
);


--
-- Name: account_emailaddress_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

ALTER TABLE public.account_emailaddress ALTER COLUMN id ADD GENERATED BY DEFAULT AS IDENTITY (
    SEQUENCE NAME public.account_emailaddress_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);


--
-- Name: account_emailconfirmation; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.account_emailconfirmation (
    id integer NOT NULL,
    created timestamp with time zone NOT NULL,
    sent timestamp with time zone,
    key character varying(64) NOT NULL,
    email_address_id integer NOT NULL
);


--
-- Name: account_emailconfirmation_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

ALTER TABLE public.account_emailconfirmation ALTER COLUMN id ADD GENERATED BY DEFAULT AS IDENTITY (
    SEQUENCE NAME public.account_emailconfirmation_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);


--
-- Name: auth_group; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.auth_group (
    id integer NOT NULL,
    name character varying(150) NOT NULL
);


--
-- Name: auth_group_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

ALTER TABLE public.auth_group ALTER COLUMN id ADD GENERATED BY DEFAULT AS IDENTITY (
    SEQUENCE NAME public.auth_group_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);


--
-- Name: auth_group_permissions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.auth_group_permissions (
    id bigint NOT NULL,
    group_id integer NOT NULL,
    permission_id integer NOT NULL
);


--
-- Name: auth_group_permissions_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

ALTER TABLE public.auth_group_permissions ALTER COLUMN id ADD GENERATED BY DEFAULT AS IDENTITY (
    SEQUENCE NAME public.auth_group_permissions_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);


--
-- Name: auth_permission; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.auth_permission (
    id integer NOT NULL,
    name character varying(255) NOT NULL,
    content_type_id integer NOT NULL,
    codename character varying(100) NOT NULL
);


--
-- Name: auth_permission_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

ALTER TABLE public.auth_permission ALTER COLUMN id ADD GENERATED BY DEFAULT AS IDENTITY (
    SEQUENCE NAME public.auth_permission_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);


--
-- Name: django_admin_log; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.django_admin_log (
    id integer NOT NULL,
    action_time timestamp with time zone NOT NULL,
    object_id text,
    object_repr character varying(200) NOT NULL,
    action_flag smallint NOT NULL,
    change_message text NOT NULL,
    content_type_id integer,
    user_id bigint NOT NULL,
    CONSTRAINT django_admin_log_action_flag_check CHECK ((action_flag >= 0))
);


--
-- Name: django_admin_log_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

ALTER TABLE public.django_admin_log ALTER COLUMN id ADD GENERATED BY DEFAULT AS IDENTITY (
    SEQUENCE NAME public.django_admin_log_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);


--
-- Name: django_content_type; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.django_content_type (
    id integer NOT NULL,
    app_label character varying(100) NOT NULL,
    model character varying(100) NOT NULL
);


--
-- Name: django_content_type_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

ALTER TABLE public.django_content_type ALTER COLUMN id ADD GENERATED BY DEFAULT AS IDENTITY (
    SEQUENCE NAME public.django_content_type_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);


--
-- Name: django_migrations; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.django_migrations (
    id bigint NOT NULL,
    app character varying(255) NOT NULL,
    name character varying(255) NOT NULL,
    applied timestamp with time zone NOT NULL
);


--
-- Name: django_migrations_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

ALTER TABLE public.django_migrations ALTER COLUMN id ADD GENERATED BY DEFAULT AS IDENTITY (
    SEQUENCE NAME public.django_migrations_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);


--
-- Name: django_q_ormq; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.django_q_ormq (
    id integer NOT NULL,
    key character varying(100) NOT NULL,
    payload text NOT NULL,
    lock timestamp with time zone
);


--
-- Name: django_q_ormq_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

ALTER TABLE public.django_q_ormq ALTER COLUMN id ADD GENERATED BY DEFAULT AS IDENTITY (
    SEQUENCE NAME public.django_q_ormq_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);


--
-- Name: django_q_schedule; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.django_q_schedule (
    id integer NOT NULL,
    func character varying(256) NOT NULL,
    hook character varying(256),
    args text,
    kwargs text,
    schedule_type character varying(2) NOT NULL,
    repeats integer NOT NULL,
    next_run timestamp with time zone,
    task character varying(100),
    name character varying(100),
    minutes smallint,
    cron character varying(100),
    cluster character varying(100),
    intended_date_kwarg character varying(100),
    CONSTRAINT django_q_schedule_minutes_check CHECK ((minutes >= 0))
);


--
-- Name: django_q_schedule_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

ALTER TABLE public.django_q_schedule ALTER COLUMN id ADD GENERATED BY DEFAULT AS IDENTITY (
    SEQUENCE NAME public.django_q_schedule_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);


--
-- Name: django_q_task; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.django_q_task (
    name character varying(100) NOT NULL,
    func character varying(256) NOT NULL,
    hook character varying(256),
    args text,
    kwargs text,
    result text,
    started timestamp with time zone NOT NULL,
    stopped timestamp with time zone NOT NULL,
    success boolean NOT NULL,
    id character varying(32) NOT NULL,
    "group" character varying(100),
    attempt_count integer NOT NULL,
    cluster character varying(100)
);


--
-- Name: django_session; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.django_session (
    session_key character varying(40) NOT NULL,
    session_data text NOT NULL,
    expire_date timestamp with time zone NOT NULL
);


--
-- Name: django_site; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.django_site (
    id integer NOT NULL,
    domain character varying(100) NOT NULL,
    name character varying(50) NOT NULL
);


--
-- Name: django_site_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

ALTER TABLE public.django_site ALTER COLUMN id ADD GENERATED BY DEFAULT AS IDENTITY (
    SEQUENCE NAME public.django_site_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);


--
-- Name: md_contribution; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.md_contribution (
    id bigint NOT NULL,
    email character varying(254),
    institution text,
    name text,
    orcid character varying(32),
    simulation_id bigint,
    rank integer NOT NULL
);


--
-- Name: md_external_link; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.md_external_link (
    id bigint NOT NULL,
    url character varying NOT NULL,
    label character varying,
    simulation_id bigint NOT NULL
);


--
-- Name: md_external_link_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

ALTER TABLE public.md_external_link ALTER COLUMN id ADD GENERATED BY DEFAULT AS IDENTITY (
    SEQUENCE NAME public.md_external_link_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);


--
-- Name: md_feature_switch; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.md_feature_switch (
    id bigint NOT NULL,
    irods_service_available boolean NOT NULL
);


--
-- Name: md_frontend_download_instance; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.md_frontend_download_instance (
    id bigint NOT NULL,
    created_on timestamp with time zone NOT NULL,
    used boolean NOT NULL,
    simulation_id bigint NOT NULL,
    user_id bigint
);


--
-- Name: md_frontend_download_instance_processed_files; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.md_frontend_download_instance_processed_files (
    id bigint NOT NULL,
    frontenddownloadinstance_id bigint NOT NULL,
    simulationprocessedfile_id bigint NOT NULL
);


--
-- Name: md_frontend_download_instance_uploaded_files; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.md_frontend_download_instance_uploaded_files (
    id bigint NOT NULL,
    frontenddownloadinstance_id bigint NOT NULL,
    simulationuploadedfile_id bigint NOT NULL
);


--
-- Name: md_ligand; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.md_ligand (
    id bigint NOT NULL,
    name text NOT NULL,
    smiles_string text NOT NULL,
    simulation_id bigint NOT NULL
);


--
-- Name: md_pdb; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.md_pdb (
    id bigint NOT NULL,
    pdb_id character varying(20) NOT NULL,
    classification character varying(255),
    title character varying(500)
);


--
-- Name: md_process_job; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.md_process_job (
    id bigint NOT NULL,
    server text NOT NULL,
    status text NOT NULL,
    log_file text,
    exit_code integer,
    last_error text,
    created_at timestamp with time zone DEFAULT statement_timestamp() NOT NULL,
    started_at timestamp with time zone,
    finished_at timestamp with time zone,
    ticket_id bigint NOT NULL
);


--
-- Name: md_process_job_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

ALTER TABLE public.md_process_job ALTER COLUMN id ADD GENERATED BY DEFAULT AS IDENTITY (
    SEQUENCE NAME public.md_process_job_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);


--
-- Name: md_processed_file; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.md_processed_file (
    id bigint NOT NULL,
    file_type character varying(40) NOT NULL,
    local_file_path character varying NOT NULL,
    filename character varying(1000) NOT NULL,
    simulation_id bigint NOT NULL,
    file_size_bytes bigint,
    description text,
    md5_hash character varying(32)
);


--
-- Name: md_pub; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.md_pub (
    id bigint NOT NULL,
    title character varying(400) NOT NULL,
    authors character varying(1000) NOT NULL,
    journal character varying(100) NOT NULL,
    volume integer NOT NULL,
    number character varying(32),
    year integer NOT NULL,
    pages character varying(100),
    doi character varying(255)
);


--
-- Name: md_replicate; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.md_replicate (
    id bigint NOT NULL,
    trajectory_file_name character varying(255) NOT NULL,
    simulation_id bigint NOT NULL
);


--
-- Name: md_replicate_group; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.md_replicate_group (
    id bigint NOT NULL,
    replicate_key character varying(255) NOT NULL,
    user_id bigint NOT NULL,
    description text NOT NULL,
    sample_mdrepo_id character varying(20) NOT NULL
);


--
-- Name: md_replicate_group_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

ALTER TABLE public.md_replicate_group ALTER COLUMN id ADD GENERATED BY DEFAULT AS IDENTITY (
    SEQUENCE NAME public.md_replicate_group_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);


--
-- Name: md_replicate_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

ALTER TABLE public.md_replicate ALTER COLUMN id ADD GENERATED BY DEFAULT AS IDENTITY (
    SEQUENCE NAME public.md_replicate_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);


--
-- Name: md_repo_app_contribution_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

ALTER TABLE public.md_contribution ALTER COLUMN id ADD GENERATED BY DEFAULT AS IDENTITY (
    SEQUENCE NAME public.md_repo_app_contribution_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);


--
-- Name: md_repo_app_featureswitch_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

ALTER TABLE public.md_feature_switch ALTER COLUMN id ADD GENERATED BY DEFAULT AS IDENTITY (
    SEQUENCE NAME public.md_repo_app_featureswitch_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);


--
-- Name: md_repo_app_frontenddownloadinstance_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

ALTER TABLE public.md_frontend_download_instance ALTER COLUMN id ADD GENERATED BY DEFAULT AS IDENTITY (
    SEQUENCE NAME public.md_repo_app_frontenddownloadinstance_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);


--
-- Name: md_repo_app_frontenddownloadinstance_processed_files_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

ALTER TABLE public.md_frontend_download_instance_processed_files ALTER COLUMN id ADD GENERATED BY DEFAULT AS IDENTITY (
    SEQUENCE NAME public.md_repo_app_frontenddownloadinstance_processed_files_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);


--
-- Name: md_repo_app_frontenddownloadinstance_uploaded_files_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

ALTER TABLE public.md_frontend_download_instance_uploaded_files ALTER COLUMN id ADD GENERATED BY DEFAULT AS IDENTITY (
    SEQUENCE NAME public.md_repo_app_frontenddownloadinstance_uploaded_files_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);


--
-- Name: md_repo_app_ligand_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

ALTER TABLE public.md_ligand ALTER COLUMN id ADD GENERATED BY DEFAULT AS IDENTITY (
    SEQUENCE NAME public.md_repo_app_ligand_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);


--
-- Name: md_ticket; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.md_ticket (
    id bigint NOT NULL,
    created_at timestamp with time zone NOT NULL,
    token character varying(40) NOT NULL,
    full_token character varying(40) NOT NULL,
    irods_tickets text,
    guid uuid NOT NULL,
    n_submissions integer NOT NULL,
    created_by_id bigint NOT NULL,
    used_for_upload boolean NOT NULL,
    irods_creation_error boolean NOT NULL,
    ticket_type character varying NOT NULL,
    no_files_found boolean NOT NULL,
    finished_generating boolean NOT NULL,
    orcid character varying(32),
    upload_notification_sent boolean NOT NULL,
    processing_complete boolean NOT NULL
);


--
-- Name: md_repo_app_mdrepoticket_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

ALTER TABLE public.md_ticket ALTER COLUMN id ADD GENERATED BY DEFAULT AS IDENTITY (
    SEQUENCE NAME public.md_repo_app_mdrepoticket_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);


--
-- Name: md_repo_app_pdb_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

ALTER TABLE public.md_pdb ALTER COLUMN id ADD GENERATED BY DEFAULT AS IDENTITY (
    SEQUENCE NAME public.md_repo_app_pdb_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);


--
-- Name: md_repo_app_pub_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

ALTER TABLE public.md_pub ALTER COLUMN id ADD GENERATED BY DEFAULT AS IDENTITY (
    SEQUENCE NAME public.md_repo_app_pub_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);


--
-- Name: md_simulation; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.md_simulation (
    id bigint NOT NULL,
    description text,
    run_commands text,
    water_type character varying(10),
    water_density double precision,
    duration double precision,
    sampling_frequency double precision,
    creation_date timestamp with time zone NOT NULL,
    software_id bigint,
    md_repo_ticket_id bigint,
    rmsd_values double precision[],
    rmsf_values double precision[],
    is_placeholder boolean NOT NULL,
    created_by_id bigint,
    unique_file_hash_string text,
    forcefield text,
    forcefield_comments text,
    temperature integer,
    is_deprecated boolean NOT NULL,
    protonation_method text,
    integration_timestep_fs integer,
    short_description text NOT NULL,
    pdb_id bigint,
    is_public boolean NOT NULL,
    fasta_sequence text,
    alias text,
    replicate_group_id bigint,
    num_replicates integer,
    is_embargoed boolean NOT NULL,
    is_coarse_grained boolean NOT NULL,
    irods_ticket character varying(255),
    superseding_simulation_id integer
);


--
-- Name: md_repo_app_simulation_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

ALTER TABLE public.md_simulation ALTER COLUMN id ADD GENERATED BY DEFAULT AS IDENTITY (
    SEQUENCE NAME public.md_repo_app_simulation_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);


--
-- Name: md_simulation_pub; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.md_simulation_pub (
    id bigint NOT NULL,
    simulation_id bigint NOT NULL,
    pub_id bigint NOT NULL
);


--
-- Name: md_repo_app_simulation_pubs_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

ALTER TABLE public.md_simulation_pub ALTER COLUMN id ADD GENERATED BY DEFAULT AS IDENTITY (
    SEQUENCE NAME public.md_repo_app_simulation_pubs_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);


--
-- Name: md_simulation_uniprot; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.md_simulation_uniprot (
    id bigint NOT NULL,
    simulation_id bigint NOT NULL,
    uniprot_id bigint NOT NULL
);


--
-- Name: md_repo_app_simulation_uniprot_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

ALTER TABLE public.md_simulation_uniprot ALTER COLUMN id ADD GENERATED BY DEFAULT AS IDENTITY (
    SEQUENCE NAME public.md_repo_app_simulation_uniprot_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);


--
-- Name: md_repo_app_simulationprocessedfile_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

ALTER TABLE public.md_processed_file ALTER COLUMN id ADD GENERATED BY DEFAULT AS IDENTITY (
    SEQUENCE NAME public.md_repo_app_simulationprocessedfile_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);


--
-- Name: md_software; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.md_software (
    id bigint NOT NULL,
    name character varying(100) NOT NULL,
    version character varying(100)
);


--
-- Name: md_repo_app_simulationsoftware_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

ALTER TABLE public.md_software ALTER COLUMN id ADD GENERATED BY DEFAULT AS IDENTITY (
    SEQUENCE NAME public.md_repo_app_simulationsoftware_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);


--
-- Name: md_uploaded_file; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.md_uploaded_file (
    id bigint NOT NULL,
    filename character varying(1000) NOT NULL,
    file_type character varying(32) NOT NULL,
    simulation_id bigint NOT NULL,
    description character varying(1000),
    local_file_path character varying NOT NULL,
    file_size_bytes bigint,
    md5_hash character varying(32),
    is_primary boolean NOT NULL
);


--
-- Name: md_repo_app_simulationuploadedfile_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

ALTER TABLE public.md_uploaded_file ALTER COLUMN id ADD GENERATED BY DEFAULT AS IDENTITY (
    SEQUENCE NAME public.md_repo_app_simulationuploadedfile_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);


--
-- Name: md_upload_instance; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.md_upload_instance (
    id bigint NOT NULL,
    created_on timestamp with time zone NOT NULL,
    simulation_id bigint,
    user_id bigint,
    successful boolean,
    lead_contributor_orcid character varying(20) NOT NULL,
    filenames text,
    ticket_id bigint,
    landing_id text
);


--
-- Name: md_repo_app_simulationuploadinstance_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

ALTER TABLE public.md_upload_instance ALTER COLUMN id ADD GENERATED BY DEFAULT AS IDENTITY (
    SEQUENCE NAME public.md_repo_app_simulationuploadinstance_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);


--
-- Name: md_upload_instance_message; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.md_upload_instance_message (
    id bigint NOT NULL,
    "timestamp" timestamp with time zone NOT NULL,
    message text NOT NULL,
    simulation_upload_id bigint NOT NULL,
    is_error boolean NOT NULL,
    is_warning boolean NOT NULL
);


--
-- Name: md_repo_app_simulationuploadstatusmessage_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

ALTER TABLE public.md_upload_instance_message ALTER COLUMN id ADD GENERATED BY DEFAULT AS IDENTITY (
    SEQUENCE NAME public.md_repo_app_simulationuploadstatusmessage_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);


--
-- Name: md_submission_completed_event; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.md_submission_completed_event (
    id bigint NOT NULL,
    created_at timestamp with time zone NOT NULL,
    path text NOT NULL
);


--
-- Name: md_repo_app_submissioncompletedevent_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

ALTER TABLE public.md_submission_completed_event ALTER COLUMN id ADD GENERATED BY DEFAULT AS IDENTITY (
    SEQUENCE NAME public.md_repo_app_submissioncompletedevent_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);


--
-- Name: md_uniprot; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.md_uniprot (
    id bigint NOT NULL,
    uniprot_id character varying(32) NOT NULL,
    name character varying(500) NOT NULL,
    amino_length integer NOT NULL,
    sequence text NOT NULL,
    CONSTRAINT md_repo_app_uniprot_amino_length_check CHECK ((amino_length >= 0))
);


--
-- Name: md_repo_app_uniprot_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

ALTER TABLE public.md_uniprot ALTER COLUMN id ADD GENERATED BY DEFAULT AS IDENTITY (
    SEQUENCE NAME public.md_repo_app_uniprot_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);


--
-- Name: md_user_groups; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.md_user_groups (
    id bigint NOT NULL,
    user_id bigint NOT NULL,
    group_id integer NOT NULL
);


--
-- Name: md_repo_app_user_groups_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

ALTER TABLE public.md_user_groups ALTER COLUMN id ADD GENERATED BY DEFAULT AS IDENTITY (
    SEQUENCE NAME public.md_repo_app_user_groups_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);


--
-- Name: md_user; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.md_user (
    id bigint NOT NULL,
    password character varying(128) NOT NULL,
    last_login timestamp with time zone,
    is_superuser boolean NOT NULL,
    username character varying(150) NOT NULL,
    is_staff boolean NOT NULL,
    date_joined timestamp with time zone NOT NULL,
    first_name character varying(50) NOT NULL,
    last_name character varying(50) NOT NULL,
    registered boolean NOT NULL,
    email character varying(254) NOT NULL,
    institution character varying(255),
    is_active boolean NOT NULL,
    can_contribute boolean NOT NULL
);


--
-- Name: md_repo_app_user_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

ALTER TABLE public.md_user ALTER COLUMN id ADD GENERATED BY DEFAULT AS IDENTITY (
    SEQUENCE NAME public.md_repo_app_user_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);


--
-- Name: md_user_user_permissions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.md_user_user_permissions (
    id bigint NOT NULL,
    user_id bigint NOT NULL,
    permission_id integer NOT NULL
);


--
-- Name: md_repo_app_user_user_permissions_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

ALTER TABLE public.md_user_user_permissions ALTER COLUMN id ADD GENERATED BY DEFAULT AS IDENTITY (
    SEQUENCE NAME public.md_repo_app_user_user_permissions_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);


--
-- Name: md_solute; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.md_solute (
    id bigint NOT NULL,
    name character varying(100) NOT NULL,
    concentration double precision NOT NULL,
    simulation_id bigint NOT NULL
);


--
-- Name: md_solute_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

ALTER TABLE public.md_solute ALTER COLUMN id ADD GENERATED BY DEFAULT AS IDENTITY (
    SEQUENCE NAME public.md_solute_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);


--
-- Name: pghistory_context; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.pghistory_context (
    id uuid NOT NULL,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL,
    metadata jsonb NOT NULL
);


--
-- Name: socialaccount_socialaccount; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.socialaccount_socialaccount (
    id integer NOT NULL,
    provider character varying(200) NOT NULL,
    uid character varying(191) NOT NULL,
    last_login timestamp with time zone NOT NULL,
    date_joined timestamp with time zone NOT NULL,
    extra_data jsonb NOT NULL,
    user_id bigint NOT NULL
);


--
-- Name: socialaccount_socialaccount_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

ALTER TABLE public.socialaccount_socialaccount ALTER COLUMN id ADD GENERATED BY DEFAULT AS IDENTITY (
    SEQUENCE NAME public.socialaccount_socialaccount_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);


--
-- Name: socialaccount_socialapp; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.socialaccount_socialapp (
    id integer NOT NULL,
    provider character varying(30) NOT NULL,
    name character varying(40) NOT NULL,
    client_id character varying(191) NOT NULL,
    secret character varying(191) NOT NULL,
    key character varying(191) NOT NULL,
    provider_id character varying(200) NOT NULL,
    settings jsonb NOT NULL
);


--
-- Name: socialaccount_socialapp_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

ALTER TABLE public.socialaccount_socialapp ALTER COLUMN id ADD GENERATED BY DEFAULT AS IDENTITY (
    SEQUENCE NAME public.socialaccount_socialapp_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);


--
-- Name: socialaccount_socialapp_sites; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.socialaccount_socialapp_sites (
    id bigint NOT NULL,
    socialapp_id integer NOT NULL,
    site_id integer NOT NULL
);


--
-- Name: socialaccount_socialapp_sites_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

ALTER TABLE public.socialaccount_socialapp_sites ALTER COLUMN id ADD GENERATED BY DEFAULT AS IDENTITY (
    SEQUENCE NAME public.socialaccount_socialapp_sites_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);


--
-- Name: socialaccount_socialtoken; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.socialaccount_socialtoken (
    id integer NOT NULL,
    token text NOT NULL,
    token_secret text NOT NULL,
    expires_at timestamp with time zone,
    account_id integer NOT NULL,
    app_id integer
);


--
-- Name: socialaccount_socialtoken_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

ALTER TABLE public.socialaccount_socialtoken ALTER COLUMN id ADD GENERATED BY DEFAULT AS IDENTITY (
    SEQUENCE NAME public.socialaccount_socialtoken_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);


--
-- Name: account_emailaddress account_emailaddress_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.account_emailaddress
    ADD CONSTRAINT account_emailaddress_pkey PRIMARY KEY (id);


--
-- Name: account_emailaddress account_emailaddress_user_id_email_987c8728_uniq; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.account_emailaddress
    ADD CONSTRAINT account_emailaddress_user_id_email_987c8728_uniq UNIQUE (user_id, email);


--
-- Name: account_emailconfirmation account_emailconfirmation_key_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.account_emailconfirmation
    ADD CONSTRAINT account_emailconfirmation_key_key UNIQUE (key);


--
-- Name: account_emailconfirmation account_emailconfirmation_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.account_emailconfirmation
    ADD CONSTRAINT account_emailconfirmation_pkey PRIMARY KEY (id);


--
-- Name: auth_group auth_group_name_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.auth_group
    ADD CONSTRAINT auth_group_name_key UNIQUE (name);


--
-- Name: auth_group_permissions auth_group_permissions_group_id_permission_id_0cd325b0_uniq; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.auth_group_permissions
    ADD CONSTRAINT auth_group_permissions_group_id_permission_id_0cd325b0_uniq UNIQUE (group_id, permission_id);


--
-- Name: auth_group_permissions auth_group_permissions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.auth_group_permissions
    ADD CONSTRAINT auth_group_permissions_pkey PRIMARY KEY (id);


--
-- Name: auth_group auth_group_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.auth_group
    ADD CONSTRAINT auth_group_pkey PRIMARY KEY (id);


--
-- Name: auth_permission auth_permission_content_type_id_codename_01ab375a_uniq; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.auth_permission
    ADD CONSTRAINT auth_permission_content_type_id_codename_01ab375a_uniq UNIQUE (content_type_id, codename);


--
-- Name: auth_permission auth_permission_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.auth_permission
    ADD CONSTRAINT auth_permission_pkey PRIMARY KEY (id);


--
-- Name: django_admin_log django_admin_log_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.django_admin_log
    ADD CONSTRAINT django_admin_log_pkey PRIMARY KEY (id);


--
-- Name: django_content_type django_content_type_app_label_model_76bd3d3b_uniq; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.django_content_type
    ADD CONSTRAINT django_content_type_app_label_model_76bd3d3b_uniq UNIQUE (app_label, model);


--
-- Name: django_content_type django_content_type_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.django_content_type
    ADD CONSTRAINT django_content_type_pkey PRIMARY KEY (id);


--
-- Name: django_migrations django_migrations_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.django_migrations
    ADD CONSTRAINT django_migrations_pkey PRIMARY KEY (id);


--
-- Name: django_q_ormq django_q_ormq_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.django_q_ormq
    ADD CONSTRAINT django_q_ormq_pkey PRIMARY KEY (id);


--
-- Name: django_q_schedule django_q_schedule_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.django_q_schedule
    ADD CONSTRAINT django_q_schedule_pkey PRIMARY KEY (id);


--
-- Name: django_q_task django_q_task_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.django_q_task
    ADD CONSTRAINT django_q_task_pkey PRIMARY KEY (id);


--
-- Name: django_session django_session_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.django_session
    ADD CONSTRAINT django_session_pkey PRIMARY KEY (session_key);


--
-- Name: django_site django_site_domain_a2e37b91_uniq; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.django_site
    ADD CONSTRAINT django_site_domain_a2e37b91_uniq UNIQUE (domain);


--
-- Name: django_site django_site_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.django_site
    ADD CONSTRAINT django_site_pkey PRIMARY KEY (id);


--
-- Name: md_external_link md_external_link_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_external_link
    ADD CONSTRAINT md_external_link_pkey PRIMARY KEY (id);


--
-- Name: md_process_job md_process_job_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_process_job
    ADD CONSTRAINT md_process_job_pkey PRIMARY KEY (id);


--
-- Name: md_replicate_group md_replicate_group_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_replicate_group
    ADD CONSTRAINT md_replicate_group_pkey PRIMARY KEY (id);


--
-- Name: md_replicate md_replicate_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_replicate
    ADD CONSTRAINT md_replicate_pkey PRIMARY KEY (id);


--
-- Name: md_contribution md_repo_app_contribution_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_contribution
    ADD CONSTRAINT md_repo_app_contribution_pkey PRIMARY KEY (id);


--
-- Name: md_feature_switch md_repo_app_featureswitch_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_feature_switch
    ADD CONSTRAINT md_repo_app_featureswitch_pkey PRIMARY KEY (id);


--
-- Name: md_frontend_download_instance_processed_files md_repo_app_frontenddown_frontenddownloadinstance_154459c2_uniq; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_frontend_download_instance_processed_files
    ADD CONSTRAINT md_repo_app_frontenddown_frontenddownloadinstance_154459c2_uniq UNIQUE (frontenddownloadinstance_id, simulationprocessedfile_id);


--
-- Name: md_frontend_download_instance_uploaded_files md_repo_app_frontenddown_frontenddownloadinstance_1be2ab75_uniq; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_frontend_download_instance_uploaded_files
    ADD CONSTRAINT md_repo_app_frontenddown_frontenddownloadinstance_1be2ab75_uniq UNIQUE (frontenddownloadinstance_id, simulationuploadedfile_id);


--
-- Name: md_frontend_download_instance md_repo_app_frontenddownloadinstance_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_frontend_download_instance
    ADD CONSTRAINT md_repo_app_frontenddownloadinstance_pkey PRIMARY KEY (id);


--
-- Name: md_frontend_download_instance_processed_files md_repo_app_frontenddownloadinstance_processed_files_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_frontend_download_instance_processed_files
    ADD CONSTRAINT md_repo_app_frontenddownloadinstance_processed_files_pkey PRIMARY KEY (id);


--
-- Name: md_frontend_download_instance_uploaded_files md_repo_app_frontenddownloadinstance_uploaded_files_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_frontend_download_instance_uploaded_files
    ADD CONSTRAINT md_repo_app_frontenddownloadinstance_uploaded_files_pkey PRIMARY KEY (id);


--
-- Name: md_ligand md_repo_app_ligand_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_ligand
    ADD CONSTRAINT md_repo_app_ligand_pkey PRIMARY KEY (id);


--
-- Name: md_ticket md_repo_app_mdrepoticket_full_token_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_ticket
    ADD CONSTRAINT md_repo_app_mdrepoticket_full_token_key UNIQUE (full_token);


--
-- Name: md_ticket md_repo_app_mdrepoticket_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_ticket
    ADD CONSTRAINT md_repo_app_mdrepoticket_pkey PRIMARY KEY (id);


--
-- Name: md_ticket md_repo_app_mdrepoticket_token_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_ticket
    ADD CONSTRAINT md_repo_app_mdrepoticket_token_key UNIQUE (token);


--
-- Name: md_pdb md_repo_app_pdb_pdb_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_pdb
    ADD CONSTRAINT md_repo_app_pdb_pdb_id_key UNIQUE (pdb_id);


--
-- Name: md_pdb md_repo_app_pdb_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_pdb
    ADD CONSTRAINT md_repo_app_pdb_pkey PRIMARY KEY (id);


--
-- Name: md_pub md_repo_app_pub_doi_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_pub
    ADD CONSTRAINT md_repo_app_pub_doi_key UNIQUE (doi);


--
-- Name: md_pub md_repo_app_pub_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_pub
    ADD CONSTRAINT md_repo_app_pub_pkey PRIMARY KEY (id);


--
-- Name: md_simulation md_repo_app_simulation_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_simulation
    ADD CONSTRAINT md_repo_app_simulation_pkey PRIMARY KEY (id);


--
-- Name: md_simulation_pub md_repo_app_simulation_pubs_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_simulation_pub
    ADD CONSTRAINT md_repo_app_simulation_pubs_pkey PRIMARY KEY (id);


--
-- Name: md_simulation_pub md_repo_app_simulation_pubs_simulation_id_pub_id_bfbdcedf_uniq; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_simulation_pub
    ADD CONSTRAINT md_repo_app_simulation_pubs_simulation_id_pub_id_bfbdcedf_uniq UNIQUE (simulation_id, pub_id);


--
-- Name: md_simulation_uniprot md_repo_app_simulation_u_simulation_id_uniprot_id_9037fbb2_uniq; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_simulation_uniprot
    ADD CONSTRAINT md_repo_app_simulation_u_simulation_id_uniprot_id_9037fbb2_uniq UNIQUE (simulation_id, uniprot_id);


--
-- Name: md_simulation_uniprot md_repo_app_simulation_uniprot_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_simulation_uniprot
    ADD CONSTRAINT md_repo_app_simulation_uniprot_pkey PRIMARY KEY (id);


--
-- Name: md_simulation md_repo_app_simulation_unique_file_hash_string_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_simulation
    ADD CONSTRAINT md_repo_app_simulation_unique_file_hash_string_key UNIQUE (unique_file_hash_string);


--
-- Name: md_processed_file md_repo_app_simulationprocessedfile_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_processed_file
    ADD CONSTRAINT md_repo_app_simulationprocessedfile_pkey PRIMARY KEY (id);


--
-- Name: md_software md_repo_app_simulationsoftware_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_software
    ADD CONSTRAINT md_repo_app_simulationsoftware_pkey PRIMARY KEY (id);


--
-- Name: md_uploaded_file md_repo_app_simulationuploadedfile_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_uploaded_file
    ADD CONSTRAINT md_repo_app_simulationuploadedfile_pkey PRIMARY KEY (id);


--
-- Name: md_upload_instance md_repo_app_simulationuploadinstance_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_upload_instance
    ADD CONSTRAINT md_repo_app_simulationuploadinstance_pkey PRIMARY KEY (id);


--
-- Name: md_upload_instance_message md_repo_app_simulationuploadstatusmessage_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_upload_instance_message
    ADD CONSTRAINT md_repo_app_simulationuploadstatusmessage_pkey PRIMARY KEY (id);


--
-- Name: md_submission_completed_event md_repo_app_submissioncompletedevent_path_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_submission_completed_event
    ADD CONSTRAINT md_repo_app_submissioncompletedevent_path_key UNIQUE (path);


--
-- Name: md_submission_completed_event md_repo_app_submissioncompletedevent_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_submission_completed_event
    ADD CONSTRAINT md_repo_app_submissioncompletedevent_pkey PRIMARY KEY (id);


--
-- Name: md_uniprot md_repo_app_uniprot_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_uniprot
    ADD CONSTRAINT md_repo_app_uniprot_pkey PRIMARY KEY (id);


--
-- Name: md_uniprot md_repo_app_uniprot_uniprot_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_uniprot
    ADD CONSTRAINT md_repo_app_uniprot_uniprot_id_key UNIQUE (uniprot_id);


--
-- Name: md_user md_repo_app_user_email_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_user
    ADD CONSTRAINT md_repo_app_user_email_key UNIQUE (email);


--
-- Name: md_user_groups md_repo_app_user_groups_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_user_groups
    ADD CONSTRAINT md_repo_app_user_groups_pkey PRIMARY KEY (id);


--
-- Name: md_user_groups md_repo_app_user_groups_user_id_group_id_ee2ac7c4_uniq; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_user_groups
    ADD CONSTRAINT md_repo_app_user_groups_user_id_group_id_ee2ac7c4_uniq UNIQUE (user_id, group_id);


--
-- Name: md_user md_repo_app_user_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_user
    ADD CONSTRAINT md_repo_app_user_pkey PRIMARY KEY (id);


--
-- Name: md_user_user_permissions md_repo_app_user_user_pe_user_id_permission_id_a949bba2_uniq; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_user_user_permissions
    ADD CONSTRAINT md_repo_app_user_user_pe_user_id_permission_id_a949bba2_uniq UNIQUE (user_id, permission_id);


--
-- Name: md_user_user_permissions md_repo_app_user_user_permissions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_user_user_permissions
    ADD CONSTRAINT md_repo_app_user_user_permissions_pkey PRIMARY KEY (id);


--
-- Name: md_user md_repo_app_user_username_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_user
    ADD CONSTRAINT md_repo_app_user_username_key UNIQUE (username);


--
-- Name: md_simulation md_simulation_irods_ticket_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_simulation
    ADD CONSTRAINT md_simulation_irods_ticket_key UNIQUE (irods_ticket);


--
-- Name: md_solute md_solute_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_solute
    ADD CONSTRAINT md_solute_pkey PRIMARY KEY (id);


--
-- Name: pghistory_context pghistory_context_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.pghistory_context
    ADD CONSTRAINT pghistory_context_pkey PRIMARY KEY (id);


--
-- Name: socialaccount_socialaccount socialaccount_socialaccount_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.socialaccount_socialaccount
    ADD CONSTRAINT socialaccount_socialaccount_pkey PRIMARY KEY (id);


--
-- Name: socialaccount_socialaccount socialaccount_socialaccount_provider_uid_fc810c6e_uniq; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.socialaccount_socialaccount
    ADD CONSTRAINT socialaccount_socialaccount_provider_uid_fc810c6e_uniq UNIQUE (provider, uid);


--
-- Name: socialaccount_socialapp_sites socialaccount_socialapp__socialapp_id_site_id_71a9a768_uniq; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.socialaccount_socialapp_sites
    ADD CONSTRAINT socialaccount_socialapp__socialapp_id_site_id_71a9a768_uniq UNIQUE (socialapp_id, site_id);


--
-- Name: socialaccount_socialapp socialaccount_socialapp_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.socialaccount_socialapp
    ADD CONSTRAINT socialaccount_socialapp_pkey PRIMARY KEY (id);


--
-- Name: socialaccount_socialapp_sites socialaccount_socialapp_sites_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.socialaccount_socialapp_sites
    ADD CONSTRAINT socialaccount_socialapp_sites_pkey PRIMARY KEY (id);


--
-- Name: socialaccount_socialtoken socialaccount_socialtoken_app_id_account_id_fca4e0ac_uniq; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.socialaccount_socialtoken
    ADD CONSTRAINT socialaccount_socialtoken_app_id_account_id_fca4e0ac_uniq UNIQUE (app_id, account_id);


--
-- Name: socialaccount_socialtoken socialaccount_socialtoken_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.socialaccount_socialtoken
    ADD CONSTRAINT socialaccount_socialtoken_pkey PRIMARY KEY (id);


--
-- Name: md_processed_file unique_processed_file_filename_per_simulation; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_processed_file
    ADD CONSTRAINT unique_processed_file_filename_per_simulation UNIQUE (filename, simulation_id);


--
-- Name: md_replicate_group unique_replicate_group_key_per_user; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_replicate_group
    ADD CONSTRAINT unique_replicate_group_key_per_user UNIQUE (user_id, replicate_key);


--
-- Name: md_replicate unique_replicate_trajectory_per_simulation; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_replicate
    ADD CONSTRAINT unique_replicate_trajectory_per_simulation UNIQUE (simulation_id, trajectory_file_name);


--
-- Name: md_simulation unique_simulation_alias_per_creator; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_simulation
    ADD CONSTRAINT unique_simulation_alias_per_creator UNIQUE (alias, created_by_id);


--
-- Name: md_uploaded_file unique_uploaded_file_filename_per_simulation; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_uploaded_file
    ADD CONSTRAINT unique_uploaded_file_filename_per_simulation UNIQUE (filename, simulation_id);


--
-- Name: account_emailaddress_email_03be32b2; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX account_emailaddress_email_03be32b2 ON public.account_emailaddress USING btree (email);


--
-- Name: account_emailaddress_email_03be32b2_like; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX account_emailaddress_email_03be32b2_like ON public.account_emailaddress USING btree (email varchar_pattern_ops);


--
-- Name: account_emailaddress_user_id_2c513194; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX account_emailaddress_user_id_2c513194 ON public.account_emailaddress USING btree (user_id);


--
-- Name: account_emailconfirmation_email_address_id_5b7f8c58; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX account_emailconfirmation_email_address_id_5b7f8c58 ON public.account_emailconfirmation USING btree (email_address_id);


--
-- Name: account_emailconfirmation_key_f43612bd_like; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX account_emailconfirmation_key_f43612bd_like ON public.account_emailconfirmation USING btree (key varchar_pattern_ops);


--
-- Name: auth_group_name_a6ea08ec_like; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX auth_group_name_a6ea08ec_like ON public.auth_group USING btree (name varchar_pattern_ops);


--
-- Name: auth_group_permissions_group_id_b120cbf9; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX auth_group_permissions_group_id_b120cbf9 ON public.auth_group_permissions USING btree (group_id);


--
-- Name: auth_group_permissions_permission_id_84c5c92e; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX auth_group_permissions_permission_id_84c5c92e ON public.auth_group_permissions USING btree (permission_id);


--
-- Name: auth_permission_content_type_id_2f476e4b; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX auth_permission_content_type_id_2f476e4b ON public.auth_permission USING btree (content_type_id);


--
-- Name: django_admin_log_content_type_id_c4bce8eb; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX django_admin_log_content_type_id_c4bce8eb ON public.django_admin_log USING btree (content_type_id);


--
-- Name: django_admin_log_user_id_c564eba6; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX django_admin_log_user_id_c564eba6 ON public.django_admin_log USING btree (user_id);


--
-- Name: django_q_task_id_32882367_like; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX django_q_task_id_32882367_like ON public.django_q_task USING btree (id varchar_pattern_ops);


--
-- Name: django_session_expire_date_a5c62663; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX django_session_expire_date_a5c62663 ON public.django_session USING btree (expire_date);


--
-- Name: django_session_session_key_c0390e0f_like; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX django_session_session_key_c0390e0f_like ON public.django_session USING btree (session_key varchar_pattern_ops);


--
-- Name: django_site_domain_a2e37b91_like; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX django_site_domain_a2e37b91_like ON public.django_site USING btree (domain varchar_pattern_ops);


--
-- Name: md_external_link_simulation_id_4debd3b3; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_external_link_simulation_id_4debd3b3 ON public.md_external_link USING btree (simulation_id);


--
-- Name: md_ligand_name_00833f_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_ligand_name_00833f_idx ON public.md_ligand USING btree (name);


--
-- Name: md_ligand_smiles__1898ae_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_ligand_smiles__1898ae_idx ON public.md_ligand USING btree (smiles_string);


--
-- Name: md_pdb_classification_trgm; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_pdb_classification_trgm ON public.md_pdb USING gin (classification public.gin_trgm_ops);


--
-- Name: md_pdb_pdb_id_trgm; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_pdb_pdb_id_trgm ON public.md_pdb USING gin (pdb_id public.gin_trgm_ops);


--
-- Name: md_pdb_title_trgm; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_pdb_title_trgm ON public.md_pdb USING gin (title public.gin_trgm_ops);


--
-- Name: md_process__status_3d4cb9_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_process__status_3d4cb9_idx ON public.md_process_job USING btree (status, created_at);


--
-- Name: md_process_job_ticket_id_590c1a63; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_process_job_ticket_id_590c1a63 ON public.md_process_job USING btree (ticket_id);


--
-- Name: md_pub_authors_fc3dc8_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_pub_authors_fc3dc8_idx ON public.md_pub USING btree (authors);


--
-- Name: md_pub_doi_a6a160_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_pub_doi_a6a160_idx ON public.md_pub USING btree (doi);


--
-- Name: md_pub_journal_0cdb82_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_pub_journal_0cdb82_idx ON public.md_pub USING btree (journal);


--
-- Name: md_pub_title_b92bc2_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_pub_title_b92bc2_idx ON public.md_pub USING btree (title);


--
-- Name: md_replicate_group_user_id_048a6a79; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_replicate_group_user_id_048a6a79 ON public.md_replicate_group USING btree (user_id);


--
-- Name: md_replicate_simulation_id_c03d294a; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_replicate_simulation_id_c03d294a ON public.md_replicate USING btree (simulation_id);


--
-- Name: md_repo_app_contribution_simulation_id_f05b770a; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_repo_app_contribution_simulation_id_f05b770a ON public.md_contribution USING btree (simulation_id);


--
-- Name: md_repo_app_frontenddownlo_frontenddownloadinstance_i_e0c92071; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_repo_app_frontenddownlo_frontenddownloadinstance_i_e0c92071 ON public.md_frontend_download_instance_processed_files USING btree (frontenddownloadinstance_id);


--
-- Name: md_repo_app_frontenddownlo_frontenddownloadinstance_i_f7653e25; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_repo_app_frontenddownlo_frontenddownloadinstance_i_f7653e25 ON public.md_frontend_download_instance_uploaded_files USING btree (frontenddownloadinstance_id);


--
-- Name: md_repo_app_frontenddownlo_simulationprocessedfile_id_838f21b6; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_repo_app_frontenddownlo_simulationprocessedfile_id_838f21b6 ON public.md_frontend_download_instance_processed_files USING btree (simulationprocessedfile_id);


--
-- Name: md_repo_app_frontenddownlo_simulationuploadedfile_id_ef83c0a1; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_repo_app_frontenddownlo_simulationuploadedfile_id_ef83c0a1 ON public.md_frontend_download_instance_uploaded_files USING btree (simulationuploadedfile_id);


--
-- Name: md_repo_app_frontenddownloadinstance_simulation_id_2f1c68dd; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_repo_app_frontenddownloadinstance_simulation_id_2f1c68dd ON public.md_frontend_download_instance USING btree (simulation_id);


--
-- Name: md_repo_app_frontenddownloadinstance_user_id_4e3be8c6; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_repo_app_frontenddownloadinstance_user_id_4e3be8c6 ON public.md_frontend_download_instance USING btree (user_id);


--
-- Name: md_repo_app_ligand_simulation_id_5826262c; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_repo_app_ligand_simulation_id_5826262c ON public.md_ligand USING btree (simulation_id);


--
-- Name: md_repo_app_mdrepoticket_created_by_id_5fa0ea1c; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_repo_app_mdrepoticket_created_by_id_5fa0ea1c ON public.md_ticket USING btree (created_by_id);


--
-- Name: md_repo_app_mdrepoticket_full_token_83a0b90a_like; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_repo_app_mdrepoticket_full_token_83a0b90a_like ON public.md_ticket USING btree (full_token varchar_pattern_ops);


--
-- Name: md_repo_app_mdrepoticket_token_4f05e222_like; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_repo_app_mdrepoticket_token_4f05e222_like ON public.md_ticket USING btree (token varchar_pattern_ops);


--
-- Name: md_repo_app_pdb_pdb_id_f541736c_like; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_repo_app_pdb_pdb_id_f541736c_like ON public.md_pdb USING btree (pdb_id varchar_pattern_ops);


--
-- Name: md_repo_app_pub_doi_2fecaec8_like; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_repo_app_pub_doi_2fecaec8_like ON public.md_pub USING btree (doi varchar_pattern_ops);


--
-- Name: md_repo_app_simulation_created_by_id_bf6777bb; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_repo_app_simulation_created_by_id_bf6777bb ON public.md_simulation USING btree (created_by_id);


--
-- Name: md_repo_app_simulation_md_repo_ticket_id_6a73be38; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_repo_app_simulation_md_repo_ticket_id_6a73be38 ON public.md_simulation USING btree (md_repo_ticket_id);


--
-- Name: md_repo_app_simulation_pdb_id_3a679c76; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_repo_app_simulation_pdb_id_3a679c76 ON public.md_simulation USING btree (pdb_id);


--
-- Name: md_repo_app_simulation_pubs_pub_id_a92edb24; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_repo_app_simulation_pubs_pub_id_a92edb24 ON public.md_simulation_pub USING btree (pub_id);


--
-- Name: md_repo_app_simulation_pubs_simulation_id_ccf7ab65; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_repo_app_simulation_pubs_simulation_id_ccf7ab65 ON public.md_simulation_pub USING btree (simulation_id);


--
-- Name: md_repo_app_simulation_software_id_242af3e0; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_repo_app_simulation_software_id_242af3e0 ON public.md_simulation USING btree (software_id);


--
-- Name: md_repo_app_simulation_uniprot_simulation_id_31f7f52c; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_repo_app_simulation_uniprot_simulation_id_31f7f52c ON public.md_simulation_uniprot USING btree (simulation_id);


--
-- Name: md_repo_app_simulation_uniprot_uniprot_id_fab58b15; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_repo_app_simulation_uniprot_uniprot_id_fab58b15 ON public.md_simulation_uniprot USING btree (uniprot_id);


--
-- Name: md_repo_app_simulation_unique_file_hash_string_0a12bf31_like; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_repo_app_simulation_unique_file_hash_string_0a12bf31_like ON public.md_simulation USING btree (unique_file_hash_string text_pattern_ops);


--
-- Name: md_repo_app_simulationprocessedfile_simulation_id_0b584a48; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_repo_app_simulationprocessedfile_simulation_id_0b584a48 ON public.md_processed_file USING btree (simulation_id);


--
-- Name: md_repo_app_simulationuplo_simulation_upload_id_419a4bf0; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_repo_app_simulationuplo_simulation_upload_id_419a4bf0 ON public.md_upload_instance_message USING btree (simulation_upload_id);


--
-- Name: md_repo_app_simulationuploadedfile_simulation_id_7c2fdb70; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_repo_app_simulationuploadedfile_simulation_id_7c2fdb70 ON public.md_uploaded_file USING btree (simulation_id);


--
-- Name: md_repo_app_simulationuploadinstance_simulation_id_0a34f055; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_repo_app_simulationuploadinstance_simulation_id_0a34f055 ON public.md_upload_instance USING btree (simulation_id);


--
-- Name: md_repo_app_simulationuploadinstance_ticket_id_e07dec51; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_repo_app_simulationuploadinstance_ticket_id_e07dec51 ON public.md_upload_instance USING btree (ticket_id);


--
-- Name: md_repo_app_simulationuploadinstance_user_id_49a1fc32; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_repo_app_simulationuploadinstance_user_id_49a1fc32 ON public.md_upload_instance USING btree (user_id);


--
-- Name: md_repo_app_submissioncompletedevent_path_967db319_like; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_repo_app_submissioncompletedevent_path_967db319_like ON public.md_submission_completed_event USING btree (path text_pattern_ops);


--
-- Name: md_repo_app_uniprot_uniprot_id_ebc02832_like; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_repo_app_uniprot_uniprot_id_ebc02832_like ON public.md_uniprot USING btree (uniprot_id varchar_pattern_ops);


--
-- Name: md_repo_app_user_email_65e0e96e_like; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_repo_app_user_email_65e0e96e_like ON public.md_user USING btree (email varchar_pattern_ops);


--
-- Name: md_repo_app_user_groups_group_id_a857a633; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_repo_app_user_groups_group_id_a857a633 ON public.md_user_groups USING btree (group_id);


--
-- Name: md_repo_app_user_groups_user_id_f6932344; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_repo_app_user_groups_user_id_f6932344 ON public.md_user_groups USING btree (user_id);


--
-- Name: md_repo_app_user_user_permissions_permission_id_b672a271; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_repo_app_user_user_permissions_permission_id_b672a271 ON public.md_user_user_permissions USING btree (permission_id);


--
-- Name: md_repo_app_user_user_permissions_user_id_e14bb326; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_repo_app_user_user_permissions_user_id_e14bb326 ON public.md_user_user_permissions USING btree (user_id);


--
-- Name: md_repo_app_user_username_99a6de0b_like; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_repo_app_user_username_99a6de0b_like ON public.md_user USING btree (username varchar_pattern_ops);


--
-- Name: md_sim_description_trgm; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_sim_description_trgm ON public.md_simulation USING gin (description public.gin_trgm_ops);


--
-- Name: md_sim_short_desc_trgm; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_sim_short_desc_trgm ON public.md_simulation USING gin (short_description public.gin_trgm_ops);


--
-- Name: md_simulati_is_plac_ffdce9_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_simulati_is_plac_ffdce9_idx ON public.md_simulation USING btree (is_placeholder);


--
-- Name: md_simulati_is_publ_60bf78_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_simulati_is_publ_60bf78_idx ON public.md_simulation USING btree (is_public);


--
-- Name: md_simulation_irods_ticket_7bd85c6f_like; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_simulation_irods_ticket_7bd85c6f_like ON public.md_simulation USING btree (irods_ticket varchar_pattern_ops);


--
-- Name: md_simulation_new_replicate_group_id_2c233b57; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_simulation_new_replicate_group_id_2c233b57 ON public.md_simulation USING btree (replicate_group_id);


--
-- Name: md_software_name_5dff06_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_software_name_5dff06_idx ON public.md_software USING btree (name);


--
-- Name: md_solute_simulation_id_954a137d; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_solute_simulation_id_954a137d ON public.md_solute USING btree (simulation_id);


--
-- Name: md_ticket_full_to_a389b0_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_ticket_full_to_a389b0_idx ON public.md_ticket USING btree (full_token);


--
-- Name: md_ticket_token_a764df_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_ticket_token_a764df_idx ON public.md_ticket USING btree (token);


--
-- Name: md_uniprot_name_9620a0_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_uniprot_name_9620a0_idx ON public.md_uniprot USING btree (name);


--
-- Name: socialaccount_socialaccount_user_id_8146e70c; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX socialaccount_socialaccount_user_id_8146e70c ON public.socialaccount_socialaccount USING btree (user_id);


--
-- Name: socialaccount_socialapp_sites_site_id_2579dee5; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX socialaccount_socialapp_sites_site_id_2579dee5 ON public.socialaccount_socialapp_sites USING btree (site_id);


--
-- Name: socialaccount_socialapp_sites_socialapp_id_97fb6e7d; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX socialaccount_socialapp_sites_socialapp_id_97fb6e7d ON public.socialaccount_socialapp_sites USING btree (socialapp_id);


--
-- Name: socialaccount_socialtoken_account_id_951f210e; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX socialaccount_socialtoken_account_id_951f210e ON public.socialaccount_socialtoken USING btree (account_id);


--
-- Name: socialaccount_socialtoken_app_id_636a42d7; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX socialaccount_socialtoken_app_id_636a42d7 ON public.socialaccount_socialtoken USING btree (app_id);


--
-- Name: success_index; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX success_index ON public.django_q_task USING btree ("group", name, func) WHERE success;


--
-- Name: unique_primary_email; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX unique_primary_email ON public.account_emailaddress USING btree (user_id, "primary") WHERE "primary";


--
-- Name: unique_verified_email; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX unique_verified_email ON public.account_emailaddress USING btree (email) WHERE verified;


--
-- Name: account_emailaddress account_emailaddress_user_id_2c513194_fk_md_repo_app_user_id; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.account_emailaddress
    ADD CONSTRAINT account_emailaddress_user_id_2c513194_fk_md_repo_app_user_id FOREIGN KEY (user_id) REFERENCES public.md_user(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: account_emailconfirmation account_emailconfirm_email_address_id_5b7f8c58_fk_account_e; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.account_emailconfirmation
    ADD CONSTRAINT account_emailconfirm_email_address_id_5b7f8c58_fk_account_e FOREIGN KEY (email_address_id) REFERENCES public.account_emailaddress(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: auth_group_permissions auth_group_permissio_permission_id_84c5c92e_fk_auth_perm; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.auth_group_permissions
    ADD CONSTRAINT auth_group_permissio_permission_id_84c5c92e_fk_auth_perm FOREIGN KEY (permission_id) REFERENCES public.auth_permission(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: auth_group_permissions auth_group_permissions_group_id_b120cbf9_fk_auth_group_id; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.auth_group_permissions
    ADD CONSTRAINT auth_group_permissions_group_id_b120cbf9_fk_auth_group_id FOREIGN KEY (group_id) REFERENCES public.auth_group(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: auth_permission auth_permission_content_type_id_2f476e4b_fk_django_co; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.auth_permission
    ADD CONSTRAINT auth_permission_content_type_id_2f476e4b_fk_django_co FOREIGN KEY (content_type_id) REFERENCES public.django_content_type(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: django_admin_log django_admin_log_content_type_id_c4bce8eb_fk_django_co; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.django_admin_log
    ADD CONSTRAINT django_admin_log_content_type_id_c4bce8eb_fk_django_co FOREIGN KEY (content_type_id) REFERENCES public.django_content_type(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: django_admin_log django_admin_log_user_id_c564eba6_fk_md_repo_app_user_id; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.django_admin_log
    ADD CONSTRAINT django_admin_log_user_id_c564eba6_fk_md_repo_app_user_id FOREIGN KEY (user_id) REFERENCES public.md_user(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_external_link md_external_link_simulation_id_4debd3b3_fk_md_simulation_id; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_external_link
    ADD CONSTRAINT md_external_link_simulation_id_4debd3b3_fk_md_simulation_id FOREIGN KEY (simulation_id) REFERENCES public.md_simulation(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_process_job md_process_job_ticket_id_590c1a63_fk_md_ticket_id; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_process_job
    ADD CONSTRAINT md_process_job_ticket_id_590c1a63_fk_md_ticket_id FOREIGN KEY (ticket_id) REFERENCES public.md_ticket(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_replicate_group md_replicate_group_user_id_048a6a79_fk_md_user_id; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_replicate_group
    ADD CONSTRAINT md_replicate_group_user_id_048a6a79_fk_md_user_id FOREIGN KEY (user_id) REFERENCES public.md_user(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_replicate md_replicate_simulation_id_c03d294a_fk_md_simulation_id; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_replicate
    ADD CONSTRAINT md_replicate_simulation_id_c03d294a_fk_md_simulation_id FOREIGN KEY (simulation_id) REFERENCES public.md_simulation(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_contribution md_repo_app_contribu_simulation_id_f05b770a_fk_md_repo_a; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_contribution
    ADD CONSTRAINT md_repo_app_contribu_simulation_id_f05b770a_fk_md_repo_a FOREIGN KEY (simulation_id) REFERENCES public.md_simulation(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_frontend_download_instance_processed_files md_repo_app_frontend_frontenddownloadinst_e0c92071_fk_md_repo_a; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_frontend_download_instance_processed_files
    ADD CONSTRAINT md_repo_app_frontend_frontenddownloadinst_e0c92071_fk_md_repo_a FOREIGN KEY (frontenddownloadinstance_id) REFERENCES public.md_frontend_download_instance(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_frontend_download_instance_uploaded_files md_repo_app_frontend_frontenddownloadinst_f7653e25_fk_md_repo_a; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_frontend_download_instance_uploaded_files
    ADD CONSTRAINT md_repo_app_frontend_frontenddownloadinst_f7653e25_fk_md_repo_a FOREIGN KEY (frontenddownloadinstance_id) REFERENCES public.md_frontend_download_instance(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_frontend_download_instance md_repo_app_frontend_simulation_id_2f1c68dd_fk_md_repo_a; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_frontend_download_instance
    ADD CONSTRAINT md_repo_app_frontend_simulation_id_2f1c68dd_fk_md_repo_a FOREIGN KEY (simulation_id) REFERENCES public.md_simulation(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_frontend_download_instance_processed_files md_repo_app_frontend_simulationprocessedf_838f21b6_fk_md_repo_a; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_frontend_download_instance_processed_files
    ADD CONSTRAINT md_repo_app_frontend_simulationprocessedf_838f21b6_fk_md_repo_a FOREIGN KEY (simulationprocessedfile_id) REFERENCES public.md_processed_file(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_frontend_download_instance_uploaded_files md_repo_app_frontend_simulationuploadedfi_ef83c0a1_fk_md_repo_a; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_frontend_download_instance_uploaded_files
    ADD CONSTRAINT md_repo_app_frontend_simulationuploadedfi_ef83c0a1_fk_md_repo_a FOREIGN KEY (simulationuploadedfile_id) REFERENCES public.md_uploaded_file(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_frontend_download_instance md_repo_app_frontend_user_id_4e3be8c6_fk_md_repo_a; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_frontend_download_instance
    ADD CONSTRAINT md_repo_app_frontend_user_id_4e3be8c6_fk_md_repo_a FOREIGN KEY (user_id) REFERENCES public.md_user(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_ligand md_repo_app_ligand_simulation_id_5826262c_fk_md_repo_a; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_ligand
    ADD CONSTRAINT md_repo_app_ligand_simulation_id_5826262c_fk_md_repo_a FOREIGN KEY (simulation_id) REFERENCES public.md_simulation(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_ticket md_repo_app_mdrepoti_created_by_id_5fa0ea1c_fk_md_repo_a; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_ticket
    ADD CONSTRAINT md_repo_app_mdrepoti_created_by_id_5fa0ea1c_fk_md_repo_a FOREIGN KEY (created_by_id) REFERENCES public.md_user(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_simulation md_repo_app_simulati_created_by_id_bf6777bb_fk_md_repo_a; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_simulation
    ADD CONSTRAINT md_repo_app_simulati_created_by_id_bf6777bb_fk_md_repo_a FOREIGN KEY (created_by_id) REFERENCES public.md_user(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_simulation md_repo_app_simulati_md_repo_ticket_id_6a73be38_fk_md_repo_a; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_simulation
    ADD CONSTRAINT md_repo_app_simulati_md_repo_ticket_id_6a73be38_fk_md_repo_a FOREIGN KEY (md_repo_ticket_id) REFERENCES public.md_ticket(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_simulation_pub md_repo_app_simulati_pub_id_a92edb24_fk_md_repo_a; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_simulation_pub
    ADD CONSTRAINT md_repo_app_simulati_pub_id_a92edb24_fk_md_repo_a FOREIGN KEY (pub_id) REFERENCES public.md_pub(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_upload_instance md_repo_app_simulati_simulation_id_0a34f055_fk_md_repo_a; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_upload_instance
    ADD CONSTRAINT md_repo_app_simulati_simulation_id_0a34f055_fk_md_repo_a FOREIGN KEY (simulation_id) REFERENCES public.md_simulation(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_processed_file md_repo_app_simulati_simulation_id_0b584a48_fk_md_repo_a; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_processed_file
    ADD CONSTRAINT md_repo_app_simulati_simulation_id_0b584a48_fk_md_repo_a FOREIGN KEY (simulation_id) REFERENCES public.md_simulation(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_simulation_uniprot md_repo_app_simulati_simulation_id_31f7f52c_fk_md_repo_a; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_simulation_uniprot
    ADD CONSTRAINT md_repo_app_simulati_simulation_id_31f7f52c_fk_md_repo_a FOREIGN KEY (simulation_id) REFERENCES public.md_simulation(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_uploaded_file md_repo_app_simulati_simulation_id_7c2fdb70_fk_md_repo_a; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_uploaded_file
    ADD CONSTRAINT md_repo_app_simulati_simulation_id_7c2fdb70_fk_md_repo_a FOREIGN KEY (simulation_id) REFERENCES public.md_simulation(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_simulation_pub md_repo_app_simulati_simulation_id_ccf7ab65_fk_md_repo_a; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_simulation_pub
    ADD CONSTRAINT md_repo_app_simulati_simulation_id_ccf7ab65_fk_md_repo_a FOREIGN KEY (simulation_id) REFERENCES public.md_simulation(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_upload_instance_message md_repo_app_simulati_simulation_upload_id_419a4bf0_fk_md_repo_a; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_upload_instance_message
    ADD CONSTRAINT md_repo_app_simulati_simulation_upload_id_419a4bf0_fk_md_repo_a FOREIGN KEY (simulation_upload_id) REFERENCES public.md_upload_instance(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_simulation md_repo_app_simulati_software_id_242af3e0_fk_md_repo_a; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_simulation
    ADD CONSTRAINT md_repo_app_simulati_software_id_242af3e0_fk_md_repo_a FOREIGN KEY (software_id) REFERENCES public.md_software(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_upload_instance md_repo_app_simulati_ticket_id_e07dec51_fk_md_repo_a; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_upload_instance
    ADD CONSTRAINT md_repo_app_simulati_ticket_id_e07dec51_fk_md_repo_a FOREIGN KEY (ticket_id) REFERENCES public.md_ticket(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_simulation_uniprot md_repo_app_simulati_uniprot_id_fab58b15_fk_md_repo_a; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_simulation_uniprot
    ADD CONSTRAINT md_repo_app_simulati_uniprot_id_fab58b15_fk_md_repo_a FOREIGN KEY (uniprot_id) REFERENCES public.md_uniprot(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_upload_instance md_repo_app_simulati_user_id_49a1fc32_fk_md_repo_a; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_upload_instance
    ADD CONSTRAINT md_repo_app_simulati_user_id_49a1fc32_fk_md_repo_a FOREIGN KEY (user_id) REFERENCES public.md_user(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_simulation md_repo_app_simulation_pdb_id_3a679c76_fk_md_repo_app_pdb_id; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_simulation
    ADD CONSTRAINT md_repo_app_simulation_pdb_id_3a679c76_fk_md_repo_app_pdb_id FOREIGN KEY (pdb_id) REFERENCES public.md_pdb(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_user_groups md_repo_app_user_groups_group_id_a857a633_fk_auth_group_id; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_user_groups
    ADD CONSTRAINT md_repo_app_user_groups_group_id_a857a633_fk_auth_group_id FOREIGN KEY (group_id) REFERENCES public.auth_group(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_user_groups md_repo_app_user_groups_user_id_f6932344_fk_md_repo_app_user_id; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_user_groups
    ADD CONSTRAINT md_repo_app_user_groups_user_id_f6932344_fk_md_repo_app_user_id FOREIGN KEY (user_id) REFERENCES public.md_user(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_user_user_permissions md_repo_app_user_use_permission_id_b672a271_fk_auth_perm; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_user_user_permissions
    ADD CONSTRAINT md_repo_app_user_use_permission_id_b672a271_fk_auth_perm FOREIGN KEY (permission_id) REFERENCES public.auth_permission(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_user_user_permissions md_repo_app_user_use_user_id_e14bb326_fk_md_repo_a; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_user_user_permissions
    ADD CONSTRAINT md_repo_app_user_use_user_id_e14bb326_fk_md_repo_a FOREIGN KEY (user_id) REFERENCES public.md_user(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_simulation md_simulation_replicate_group_id_f66b7e84_fk_md_replic; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_simulation
    ADD CONSTRAINT md_simulation_replicate_group_id_f66b7e84_fk_md_replic FOREIGN KEY (replicate_group_id) REFERENCES public.md_replicate_group(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_solute md_solute_simulation_id_954a137d_fk_md_simulation_id; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_solute
    ADD CONSTRAINT md_solute_simulation_id_954a137d_fk_md_simulation_id FOREIGN KEY (simulation_id) REFERENCES public.md_simulation(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: socialaccount_socialtoken socialaccount_social_account_id_951f210e_fk_socialacc; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.socialaccount_socialtoken
    ADD CONSTRAINT socialaccount_social_account_id_951f210e_fk_socialacc FOREIGN KEY (account_id) REFERENCES public.socialaccount_socialaccount(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: socialaccount_socialtoken socialaccount_social_app_id_636a42d7_fk_socialacc; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.socialaccount_socialtoken
    ADD CONSTRAINT socialaccount_social_app_id_636a42d7_fk_socialacc FOREIGN KEY (app_id) REFERENCES public.socialaccount_socialapp(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: socialaccount_socialapp_sites socialaccount_social_site_id_2579dee5_fk_django_si; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.socialaccount_socialapp_sites
    ADD CONSTRAINT socialaccount_social_site_id_2579dee5_fk_django_si FOREIGN KEY (site_id) REFERENCES public.django_site(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: socialaccount_socialapp_sites socialaccount_social_socialapp_id_97fb6e7d_fk_socialacc; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.socialaccount_socialapp_sites
    ADD CONSTRAINT socialaccount_social_socialapp_id_97fb6e7d_fk_socialacc FOREIGN KEY (socialapp_id) REFERENCES public.socialaccount_socialapp(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: socialaccount_socialaccount socialaccount_socialaccount_user_id_8146e70c_fk_md_user_id; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.socialaccount_socialaccount
    ADD CONSTRAINT socialaccount_socialaccount_user_id_8146e70c_fk_md_user_id FOREIGN KEY (user_id) REFERENCES public.md_user(id) DEFERRABLE INITIALLY DEFERRED;


--
-- PostgreSQL database dump complete
--

\unrestrict t8Epape6afZJHaLtSO2kmPoVZuzZobOU4FXTGNaP49CD87mUjqcz0Darflt6fM8

