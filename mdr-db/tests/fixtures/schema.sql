-- Test-database schema fixture for the mdr-db integration tests.
--
-- This is a schema-only (no data) snapshot of the Django-owned mdrepo schema.
-- The schema is owned by the Django models in md-repo-app, NOT by diesel, so
-- this file is a point-in-time snapshot and will drift when Django migrations
-- change the schema. Regenerate it with a schema-only dump of a database at
-- the migration you need:
--
--   pg_dump --schema-only --no-owner --no-privileges "$DSN" \
--     > mdr-db/tests/fixtures/schema.sql
--
-- then re-add this header block and the two INSERTs for the file-type lookups
-- at the end, which every file row references.
--
-- This snapshot (2026-10-09): the 2026-10-01 snapshot (staging's schema at
-- 0285 plus md-repo-app 0286) with 0287 and 0288 applied (`sqlmigrate
-- md_repo_app 0287`/`0288`: md_chain's UniProt reference and hit, its `pdb`
-- match method, raw API responses on md_uniprot and md_pdb), dumped from the
-- postgres:16 test container.
--
--
-- PostgreSQL database dump
--

\restrict Ho7gvdbAA10Td0iQRZ1AnnfhxbAiE6USBpT0vsTDha9yL0PrLo8p8f9hWy9kc3e

-- Dumped from database version 16.14 (Debian 16.14-1.pgdg13+1)
-- Dumped by pg_dump version 16.14 (Debian 16.14-1.pgdg13+1)

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
-- Name: md_chain; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.md_chain (
    id bigint NOT NULL,
    chain_order integer NOT NULL,
    chain_label character varying(4) NOT NULL,
    source character varying(16) NOT NULL,
    first_residue integer,
    last_residue integer,
    n_terminal_cap character varying(5),
    c_terminal_cap character varying(5),
    match_method character varying(16),
    reference_start integer,
    reference_end integer,
    reference_identity double precision,
    reference_coverage double precision,
    simulation_id bigint NOT NULL,
    polymer_id bigint NOT NULL,
    identity double precision,
    query_end integer,
    query_start integer,
    uniprot_id bigint,
    CONSTRAINT md_chain_chain_order CHECK ((chain_order > 0)),
    CONSTRAINT md_chain_match_method CHECK (((match_method IS NULL) OR ((match_method)::text = ANY (ARRAY[('pdb'::character varying)::text, ('declared'::character varying)::text, ('aligned'::character varying)::text, ('none'::character varying)::text])))),
    CONSTRAINT md_chain_source CHECK (((source)::text = ANY (ARRAY[('structure'::character varying)::text, ('declared'::character varying)::text]))),
    CONSTRAINT md_chain_uniprot_hit CHECK ((((identity IS NULL) AND (query_end IS NULL) AND (query_start IS NULL) AND (reference_end IS NULL) AND (reference_start IS NULL) AND (uniprot_id IS NULL) AND ((match_method IS NULL) OR ((match_method)::text = 'none'::text))) OR ((identity IS NOT NULL) AND ((match_method)::text = ANY ((ARRAY['pdb'::character varying, 'declared'::character varying, 'aligned'::character varying])::text[])) AND (match_method IS NOT NULL) AND (query_end IS NOT NULL) AND (query_start IS NOT NULL) AND (reference_end IS NOT NULL) AND (reference_start IS NOT NULL) AND (uniprot_id IS NOT NULL) AND (query_start >= 1) AND (query_end >= query_start) AND (reference_start >= 1) AND (reference_end >= reference_start) AND (identity >= (0.0)::double precision) AND (identity <= (100.0)::double precision))))
);


--
-- Name: md_chain_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

ALTER TABLE public.md_chain ALTER COLUMN id ADD GENERATED BY DEFAULT AS IDENTITY (
    SEQUENCE NAME public.md_chain_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);


--
-- Name: md_collection; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.md_collection (
    id bigint NOT NULL,
    name character varying(255) NOT NULL,
    description text,
    user_id bigint NOT NULL
);


--
-- Name: md_collection_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

ALTER TABLE public.md_collection ALTER COLUMN id ADD GENERATED BY DEFAULT AS IDENTITY (
    SEQUENCE NAME public.md_collection_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);


--
-- Name: md_creator; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.md_creator (
    id bigint NOT NULL,
    name text,
    orcid character varying(32),
    email character varying(254),
    institution text
);


--
-- Name: md_creator_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

ALTER TABLE public.md_creator ALTER COLUMN id ADD GENERATED BY DEFAULT AS IDENTITY (
    SEQUENCE NAME public.md_creator_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
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
-- Name: md_favorite; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.md_favorite (
    id bigint NOT NULL,
    created_on timestamp with time zone NOT NULL,
    simulation_id bigint NOT NULL,
    user_id bigint NOT NULL
);


--
-- Name: md_favorite_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

ALTER TABLE public.md_favorite ALTER COLUMN id ADD GENERATED BY DEFAULT AS IDENTITY (
    SEQUENCE NAME public.md_favorite_id_seq
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
    smiles text,
    simulation_id bigint NOT NULL,
    declared_identity text,
    identity_software text,
    inchi text,
    inchikey text,
    chain_id bigint,
    CONSTRAINT md_ligand_smiles_or_chain CHECK (((smiles IS NOT NULL) OR (chain_id IS NOT NULL)))
);


--
-- Name: md_pdb; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.md_pdb (
    id bigint NOT NULL,
    pdb_id character varying(20) NOT NULL,
    classification character varying(255),
    title character varying(500),
    response jsonb,
    entities_response jsonb,
    fetched_at timestamp with time zone
);


--
-- Name: md_polymer; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.md_polymer (
    id bigint NOT NULL,
    polymer_type character varying(8) NOT NULL,
    sequence text NOT NULL,
    residues character varying(5)[] NOT NULL,
    residues_hash character varying(64) NOT NULL,
    num_residues integer NOT NULL,
    reference_db character varying(16),
    reference_accession character varying(32),
    CONSTRAINT md_polymer_num_residues CHECK (((num_residues > 0) AND (num_residues = cardinality(residues)) AND (num_residues = char_length(sequence)))),
    CONSTRAINT md_polymer_polymer_type CHECK (((polymer_type)::text = ANY (ARRAY[('protein'::character varying)::text, ('dna'::character varying)::text, ('rna'::character varying)::text]))),
    CONSTRAINT md_polymer_reference_db CHECK (((reference_db IS NULL) OR ((reference_db)::text = ANY (ARRAY[('uniprot'::character varying)::text, ('rnacentral'::character varying)::text])))),
    CONSTRAINT md_polymer_reference_pair CHECK ((((reference_accession IS NULL) AND (reference_db IS NULL)) OR ((reference_accession IS NOT NULL) AND (reference_db IS NOT NULL)))),
    CONSTRAINT md_polymer_residues_hash_format CHECK (((residues_hash)::text ~ '^[0-9a-f]{64}$'::text))
);


--
-- Name: md_polymer_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

ALTER TABLE public.md_polymer ALTER COLUMN id ADD GENERATED BY DEFAULT AS IDENTITY (
    SEQUENCE NAME public.md_polymer_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
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
    ticket_id bigint NOT NULL,
    num_attempts integer DEFAULT 0 NOT NULL
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
    md5_hash character varying(32),
    CONSTRAINT md_processed_file_md5_hash_format CHECK (((md5_hash IS NULL) OR ((md5_hash)::text ~ '^[0-9a-f]{32}$'::text)))
);


--
-- Name: md_processed_file_type; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.md_processed_file_type (
    name character varying(40) NOT NULL
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
    processing_complete boolean NOT NULL,
    irods_creation_error_detail text
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
    contributor_id bigint,
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
    num_replicates integer,
    is_embargoed boolean NOT NULL,
    is_coarse_grained boolean NOT NULL,
    irods_ticket character varying(255),
    superseding_simulation_id integer,
    sampling_frequency_ps double precision
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
    is_primary boolean NOT NULL,
    CONSTRAINT md_uploaded_file_md5_hash_format CHECK (((md5_hash IS NULL) OR ((md5_hash)::text ~ '^[0-9a-f]{32}$'::text)))
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
    landing_id text,
    is_abandoned boolean DEFAULT false NOT NULL
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
    response jsonb,
    entry_version integer,
    fetched_at timestamp with time zone,
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
-- Name: md_simulation_collection; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.md_simulation_collection (
    id bigint NOT NULL,
    simulation_id bigint NOT NULL,
    collection_id bigint NOT NULL
);


--
-- Name: md_simulation_collection_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

ALTER TABLE public.md_simulation_collection ALTER COLUMN id ADD GENERATED BY DEFAULT AS IDENTITY (
    SEQUENCE NAME public.md_simulation_collection_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);


--
-- Name: md_simulation_creator; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.md_simulation_creator (
    rank integer NOT NULL,
    creator_id bigint NOT NULL,
    simulation_id bigint NOT NULL
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
-- Name: md_triage_delivery; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.md_triage_delivery (
    id bigint NOT NULL,
    server text NOT NULL,
    channel text NOT NULL,
    role text NOT NULL,
    idempotency_key text NOT NULL,
    body text NOT NULL,
    status text NOT NULL,
    slack_ts text,
    attempt_count integer NOT NULL,
    last_error text,
    posting_started_at timestamp with time zone,
    delivered_at timestamp with time zone,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL,
    parent_id bigint,
    ticket_id bigint,
    CONSTRAINT md_triage_delivery_attempt_count_check CHECK ((attempt_count >= 0))
);


--
-- Name: md_triage_delivery_finding; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.md_triage_delivery_finding (
    id bigint NOT NULL,
    delivery_id bigint NOT NULL,
    finding_id bigint NOT NULL
);


--
-- Name: md_triage_delivery_finding_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

ALTER TABLE public.md_triage_delivery_finding ALTER COLUMN id ADD GENERATED BY DEFAULT AS IDENTITY (
    SEQUENCE NAME public.md_triage_delivery_finding_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);


--
-- Name: md_triage_delivery_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

ALTER TABLE public.md_triage_delivery ALTER COLUMN id ADD GENERATED BY DEFAULT AS IDENTITY (
    SEQUENCE NAME public.md_triage_delivery_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);


--
-- Name: md_triage_finding; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.md_triage_finding (
    id bigint NOT NULL,
    server text NOT NULL,
    evidence_kind text NOT NULL,
    landing_id text,
    event_key text NOT NULL,
    normalized_signature text NOT NULL,
    raw_hash text NOT NULL,
    phase text NOT NULL,
    failure_class text NOT NULL,
    cause_domain text NOT NULL,
    next_actor text NOT NULL,
    confidence text NOT NULL,
    needs_review boolean NOT NULL,
    review_reason text,
    rule_ids jsonb NOT NULL,
    remediation text NOT NULL,
    evidence jsonb NOT NULL,
    state text NOT NULL,
    first_seen_at timestamp with time zone NOT NULL,
    last_seen_at timestamp with time zone NOT NULL,
    process_job_id bigint,
    ticket_id bigint,
    upload_instance_id bigint
);


--
-- Name: md_triage_finding_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

ALTER TABLE public.md_triage_finding ALTER COLUMN id ADD GENERATED BY DEFAULT AS IDENTITY (
    SEQUENCE NAME public.md_triage_finding_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);


--
-- Name: md_triage_model_review; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.md_triage_model_review (
    review_key character varying(64) NOT NULL,
    failure_class text NOT NULL,
    cause_domain text NOT NULL,
    next_actor text NOT NULL,
    confidence text NOT NULL,
    needs_review boolean NOT NULL,
    review_reason text,
    remediation text NOT NULL,
    explanation text,
    model_name text,
    input_tokens bigint,
    output_tokens bigint,
    created_at timestamp with time zone NOT NULL,
    CONSTRAINT md_triage_model_review_input_tokens_check CHECK ((input_tokens >= 0)),
    CONSTRAINT md_triage_model_review_output_tokens_check CHECK ((output_tokens >= 0))
);


--
-- Name: md_triage_occurrence; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.md_triage_occurrence (
    id bigint NOT NULL,
    observed_at timestamp with time zone NOT NULL,
    raw_hash text NOT NULL,
    source_generation text NOT NULL,
    evidence jsonb NOT NULL,
    finding_id bigint NOT NULL
);


--
-- Name: md_triage_occurrence_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

ALTER TABLE public.md_triage_occurrence ALTER COLUMN id ADD GENERATED BY DEFAULT AS IDENTITY (
    SEQUENCE NAME public.md_triage_occurrence_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);


--
-- Name: md_triage_reaction; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.md_triage_reaction (
    id bigint NOT NULL,
    reaction text NOT NULL,
    slack_user_id text NOT NULL,
    active boolean NOT NULL,
    first_seen_at timestamp with time zone NOT NULL,
    last_seen_at timestamp with time zone NOT NULL,
    delivery_id bigint NOT NULL
);


--
-- Name: md_triage_reaction_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

ALTER TABLE public.md_triage_reaction ALTER COLUMN id ADD GENERATED BY DEFAULT AS IDENTITY (
    SEQUENCE NAME public.md_triage_reaction_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);


--
-- Name: md_uploaded_file_type; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.md_uploaded_file_type (
    name character varying(32) NOT NULL
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
-- Name: md_chain md_chain_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_chain
    ADD CONSTRAINT md_chain_pkey PRIMARY KEY (id);


--
-- Name: md_chain md_chain_unique_order; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_chain
    ADD CONSTRAINT md_chain_unique_order UNIQUE (simulation_id, chain_order);


--
-- Name: md_chain md_chain_unique_simulation_id; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_chain
    ADD CONSTRAINT md_chain_unique_simulation_id UNIQUE (simulation_id, id);


--
-- Name: md_collection md_collection_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_collection
    ADD CONSTRAINT md_collection_pkey PRIMARY KEY (id);


--
-- Name: md_creator md_creator_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_creator
    ADD CONSTRAINT md_creator_pkey PRIMARY KEY (id);


--
-- Name: md_external_link md_external_link_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_external_link
    ADD CONSTRAINT md_external_link_pkey PRIMARY KEY (id);


--
-- Name: md_favorite md_favorite_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_favorite
    ADD CONSTRAINT md_favorite_pkey PRIMARY KEY (id);


--
-- Name: md_ligand md_ligand_chain_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_ligand
    ADD CONSTRAINT md_ligand_chain_id_key UNIQUE (chain_id);


--
-- Name: md_polymer md_polymer_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_polymer
    ADD CONSTRAINT md_polymer_pkey PRIMARY KEY (id);


--
-- Name: md_polymer md_polymer_unique_residues; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_polymer
    ADD CONSTRAINT md_polymer_unique_residues UNIQUE (polymer_type, residues_hash);


--
-- Name: md_process_job md_process_job_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_process_job
    ADD CONSTRAINT md_process_job_pkey PRIMARY KEY (id);


--
-- Name: md_processed_file_type md_processed_file_type_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_processed_file_type
    ADD CONSTRAINT md_processed_file_type_pkey PRIMARY KEY (name);


--
-- Name: md_replicate md_replicate_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_replicate
    ADD CONSTRAINT md_replicate_pkey PRIMARY KEY (id);


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
-- Name: md_simulation_collection md_simulation_collection_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_simulation_collection
    ADD CONSTRAINT md_simulation_collection_pkey PRIMARY KEY (id);


--
-- Name: md_simulation_collection md_simulation_collection_simulation_id_collection_6338d716_uniq; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_simulation_collection
    ADD CONSTRAINT md_simulation_collection_simulation_id_collection_6338d716_uniq UNIQUE (simulation_id, collection_id);


--
-- Name: md_simulation_creator md_simulation_creator_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_simulation_creator
    ADD CONSTRAINT md_simulation_creator_pkey PRIMARY KEY (simulation_id, creator_id);


--
-- Name: md_simulation md_simulation_irods_ticket_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_simulation
    ADD CONSTRAINT md_simulation_irods_ticket_key UNIQUE (irods_ticket);


--
-- Name: md_software md_software_name_version_uniq; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_software
    ADD CONSTRAINT md_software_name_version_uniq UNIQUE (name, version);


--
-- Name: md_solute md_solute_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_solute
    ADD CONSTRAINT md_solute_pkey PRIMARY KEY (id);


--
-- Name: md_triage_delivery_finding md_triage_delivery_finding_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_triage_delivery_finding
    ADD CONSTRAINT md_triage_delivery_finding_pkey PRIMARY KEY (id);


--
-- Name: md_triage_delivery_finding md_triage_delivery_finding_uniq; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_triage_delivery_finding
    ADD CONSTRAINT md_triage_delivery_finding_uniq UNIQUE (delivery_id, finding_id);


--
-- Name: md_triage_delivery md_triage_delivery_idempotency_key_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_triage_delivery
    ADD CONSTRAINT md_triage_delivery_idempotency_key_key UNIQUE (idempotency_key);


--
-- Name: md_triage_delivery md_triage_delivery_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_triage_delivery
    ADD CONSTRAINT md_triage_delivery_pkey PRIMARY KEY (id);


--
-- Name: md_triage_finding md_triage_finding_event_key_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_triage_finding
    ADD CONSTRAINT md_triage_finding_event_key_key UNIQUE (event_key);


--
-- Name: md_triage_finding md_triage_finding_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_triage_finding
    ADD CONSTRAINT md_triage_finding_pkey PRIMARY KEY (id);


--
-- Name: md_triage_model_review md_triage_model_review_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_triage_model_review
    ADD CONSTRAINT md_triage_model_review_pkey PRIMARY KEY (review_key);


--
-- Name: md_triage_occurrence md_triage_occurrence_generation_uniq; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_triage_occurrence
    ADD CONSTRAINT md_triage_occurrence_generation_uniq UNIQUE (finding_id, source_generation, raw_hash);


--
-- Name: md_triage_occurrence md_triage_occurrence_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_triage_occurrence
    ADD CONSTRAINT md_triage_occurrence_pkey PRIMARY KEY (id);


--
-- Name: md_triage_reaction md_triage_reaction_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_triage_reaction
    ADD CONSTRAINT md_triage_reaction_pkey PRIMARY KEY (id);


--
-- Name: md_triage_reaction md_triage_reaction_user_uniq; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_triage_reaction
    ADD CONSTRAINT md_triage_reaction_user_uniq UNIQUE (delivery_id, reaction, slack_user_id);


--
-- Name: md_uploaded_file_type md_uploaded_file_type_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_uploaded_file_type
    ADD CONSTRAINT md_uploaded_file_type_pkey PRIMARY KEY (name);


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
-- Name: md_collection unique_collection_name_per_user; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_collection
    ADD CONSTRAINT unique_collection_name_per_user UNIQUE (user_id, name);


--
-- Name: md_favorite unique_favorite_per_user_simulation; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_favorite
    ADD CONSTRAINT unique_favorite_per_user_simulation UNIQUE (user_id, simulation_id);


--
-- Name: md_processed_file unique_processed_file_filename_per_simulation; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_processed_file
    ADD CONSTRAINT unique_processed_file_filename_per_simulation UNIQUE (filename, simulation_id);


--
-- Name: md_replicate unique_replicate_trajectory_per_simulation; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_replicate
    ADD CONSTRAINT unique_replicate_trajectory_per_simulation UNIQUE (simulation_id, trajectory_file_name);


--
-- Name: md_simulation unique_simulation_alias_per_creator; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_simulation
    ADD CONSTRAINT unique_simulation_alias_per_creator UNIQUE (alias, contributor_id);


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
-- Name: md_chain_polymer_id_447b5258; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_chain_polymer_id_447b5258 ON public.md_chain USING btree (polymer_id);


--
-- Name: md_chain_simulation_id_0bd58087; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_chain_simulation_id_0bd58087 ON public.md_chain USING btree (simulation_id);


--
-- Name: md_chain_uniprot_id_e122a0fa; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_chain_uniprot_id_e122a0fa ON public.md_chain USING btree (uniprot_id);


--
-- Name: md_collection_user_id_0e5b87db; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_collection_user_id_0e5b87db ON public.md_collection USING btree (user_id);


--
-- Name: md_external_link_simulation_id_4debd3b3; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_external_link_simulation_id_4debd3b3 ON public.md_external_link USING btree (simulation_id);


--
-- Name: md_favorite_simulation_id_99548276; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_favorite_simulation_id_99548276 ON public.md_favorite USING btree (simulation_id);


--
-- Name: md_favorite_user_id_107955df; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_favorite_user_id_107955df ON public.md_favorite USING btree (user_id);


--
-- Name: md_ligand_inchi_trgm; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_ligand_inchi_trgm ON public.md_ligand USING gin (upper(inchi) public.gin_trgm_ops);


--
-- Name: md_ligand_inchikey_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_ligand_inchikey_idx ON public.md_ligand USING btree (inchikey);


--
-- Name: md_ligand_name_trgm; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_ligand_name_trgm ON public.md_ligand USING gin (upper(name) public.gin_trgm_ops);


--
-- Name: md_ligand_smiles_trgm; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_ligand_smiles_trgm ON public.md_ligand USING gin (upper(smiles) public.gin_trgm_ops);


--
-- Name: md_pdb_classification_trgm; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_pdb_classification_trgm ON public.md_pdb USING gin (upper((classification)::text) public.gin_trgm_ops);


--
-- Name: md_pdb_pdb_id_trgm; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_pdb_pdb_id_trgm ON public.md_pdb USING gin (upper((pdb_id)::text) public.gin_trgm_ops);


--
-- Name: md_pdb_title_trgm; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_pdb_title_trgm ON public.md_pdb USING gin (upper((title)::text) public.gin_trgm_ops);


--
-- Name: md_polymer_accession_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_polymer_accession_idx ON public.md_polymer USING btree (reference_accession);


--
-- Name: md_process__status_3d4cb9_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_process__status_3d4cb9_idx ON public.md_process_job USING btree (status, created_at);


--
-- Name: md_process_job_ticket_id_590c1a63; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_process_job_ticket_id_590c1a63 ON public.md_process_job USING btree (ticket_id);


--
-- Name: md_processed_file_type_name_2e0dbc00_like; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_processed_file_type_name_2e0dbc00_like ON public.md_processed_file_type USING btree (name varchar_pattern_ops);


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
-- Name: md_replicate_simulation_id_c03d294a; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_replicate_simulation_id_c03d294a ON public.md_replicate USING btree (simulation_id);


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

CREATE INDEX md_repo_app_simulation_created_by_id_bf6777bb ON public.md_simulation USING btree (contributor_id);


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

CREATE INDEX md_sim_description_trgm ON public.md_simulation USING gin (upper(description) public.gin_trgm_ops);


--
-- Name: md_sim_short_desc_trgm; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_sim_short_desc_trgm ON public.md_simulation USING gin (upper(short_description) public.gin_trgm_ops);


--
-- Name: md_simulati_is_plac_ffdce9_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_simulati_is_plac_ffdce9_idx ON public.md_simulation USING btree (is_placeholder);


--
-- Name: md_simulati_is_publ_60bf78_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_simulati_is_publ_60bf78_idx ON public.md_simulation USING btree (is_public);


--
-- Name: md_simulation_collection_collection_id_c0931a52; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_simulation_collection_collection_id_c0931a52 ON public.md_simulation_collection USING btree (collection_id);


--
-- Name: md_simulation_collection_simulation_id_d01829fc; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_simulation_collection_simulation_id_d01829fc ON public.md_simulation_collection USING btree (simulation_id);


--
-- Name: md_simulation_creator_creator_id_12d0500b; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_simulation_creator_creator_id_12d0500b ON public.md_simulation_creator USING btree (creator_id);


--
-- Name: md_simulation_irods_ticket_7bd85c6f_like; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_simulation_irods_ticket_7bd85c6f_like ON public.md_simulation USING btree (irods_ticket varchar_pattern_ops);


--
-- Name: md_software_name_trgm; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_software_name_trgm ON public.md_software USING gin (upper((name)::text) public.gin_trgm_ops);


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
-- Name: md_triage_d_server_9b8263_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_triage_d_server_9b8263_idx ON public.md_triage_delivery USING btree (server, ticket_id, role);


--
-- Name: md_triage_d_status_6cd5f0_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_triage_d_status_6cd5f0_idx ON public.md_triage_delivery USING btree (status, created_at);


--
-- Name: md_triage_delivery_finding_delivery_id_ca26c333; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_triage_delivery_finding_delivery_id_ca26c333 ON public.md_triage_delivery_finding USING btree (delivery_id);


--
-- Name: md_triage_delivery_finding_finding_id_9cf41ab5; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_triage_delivery_finding_finding_id_9cf41ab5 ON public.md_triage_delivery_finding USING btree (finding_id);


--
-- Name: md_triage_delivery_idempotency_key_b47aea2e_like; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_triage_delivery_idempotency_key_b47aea2e_like ON public.md_triage_delivery USING btree (idempotency_key text_pattern_ops);


--
-- Name: md_triage_delivery_parent_id_0ea2800a; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_triage_delivery_parent_id_0ea2800a ON public.md_triage_delivery USING btree (parent_id);


--
-- Name: md_triage_delivery_ticket_id_f71e7ec6; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_triage_delivery_ticket_id_f71e7ec6 ON public.md_triage_delivery USING btree (ticket_id);


--
-- Name: md_triage_f_server_3bc161_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_triage_f_server_3bc161_idx ON public.md_triage_finding USING btree (server, state, last_seen_at);


--
-- Name: md_triage_f_ticket__16019c_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_triage_f_ticket__16019c_idx ON public.md_triage_finding USING btree (ticket_id, state);


--
-- Name: md_triage_finding_event_key_c04b561b_like; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_triage_finding_event_key_c04b561b_like ON public.md_triage_finding USING btree (event_key text_pattern_ops);


--
-- Name: md_triage_finding_process_job_id_424ed3a1; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_triage_finding_process_job_id_424ed3a1 ON public.md_triage_finding USING btree (process_job_id);


--
-- Name: md_triage_finding_ticket_id_13c8c123; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_triage_finding_ticket_id_13c8c123 ON public.md_triage_finding USING btree (ticket_id);


--
-- Name: md_triage_finding_upload_instance_id_c5f1bcf0; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_triage_finding_upload_instance_id_c5f1bcf0 ON public.md_triage_finding USING btree (upload_instance_id);


--
-- Name: md_triage_model_review_review_key_79e1ff82_like; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_triage_model_review_review_key_79e1ff82_like ON public.md_triage_model_review USING btree (review_key varchar_pattern_ops);


--
-- Name: md_triage_occurrence_finding_id_73e4a591; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_triage_occurrence_finding_id_73e4a591 ON public.md_triage_occurrence USING btree (finding_id);


--
-- Name: md_triage_reaction_delivery_id_04ad9529; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_triage_reaction_delivery_id_04ad9529 ON public.md_triage_reaction USING btree (delivery_id);


--
-- Name: md_uniprot_id_trgm; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_uniprot_id_trgm ON public.md_uniprot USING gin (upper((uniprot_id)::text) public.gin_trgm_ops);


--
-- Name: md_uniprot_name_trgm; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_uniprot_name_trgm ON public.md_uniprot USING gin (upper((name)::text) public.gin_trgm_ops);


--
-- Name: md_uploaded_file_type_name_a3868635_like; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX md_uploaded_file_type_name_a3868635_like ON public.md_uploaded_file_type USING btree (name varchar_pattern_ops);


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
-- Name: uniq_creator_identity; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX uniq_creator_identity ON public.md_creator USING btree (lower(COALESCE(name, ''::text)), COALESCE(orcid, ''::character varying), lower((COALESCE(email, ''::character varying))::text), lower(COALESCE(institution, ''::text)));


--
-- Name: unique_primary_email; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX unique_primary_email ON public.account_emailaddress USING btree (user_id, "primary") WHERE "primary";


--
-- Name: unique_primary_file_per_type_per_simulation; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX unique_primary_file_per_type_per_simulation ON public.md_uploaded_file USING btree (simulation_id, file_type) WHERE is_primary;


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
-- Name: md_chain md_chain_polymer_id_447b5258_fk_md_polymer_id; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_chain
    ADD CONSTRAINT md_chain_polymer_id_447b5258_fk_md_polymer_id FOREIGN KEY (polymer_id) REFERENCES public.md_polymer(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_chain md_chain_simulation_id_0bd58087_fk_md_simulation_id; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_chain
    ADD CONSTRAINT md_chain_simulation_id_0bd58087_fk_md_simulation_id FOREIGN KEY (simulation_id) REFERENCES public.md_simulation(id) ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_chain md_chain_uniprot_id_e122a0fa_fk_md_uniprot_id; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_chain
    ADD CONSTRAINT md_chain_uniprot_id_e122a0fa_fk_md_uniprot_id FOREIGN KEY (uniprot_id) REFERENCES public.md_uniprot(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_collection md_collection_user_id_0e5b87db_fk_md_user_id; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_collection
    ADD CONSTRAINT md_collection_user_id_0e5b87db_fk_md_user_id FOREIGN KEY (user_id) REFERENCES public.md_user(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_external_link md_external_link_simulation_id_4debd3b3_fk_md_simulation_id; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_external_link
    ADD CONSTRAINT md_external_link_simulation_id_4debd3b3_fk_md_simulation_id FOREIGN KEY (simulation_id) REFERENCES public.md_simulation(id) ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_favorite md_favorite_simulation_id_99548276_fk_md_simulation_id; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_favorite
    ADD CONSTRAINT md_favorite_simulation_id_99548276_fk_md_simulation_id FOREIGN KEY (simulation_id) REFERENCES public.md_simulation(id) ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_favorite md_favorite_user_id_107955df_fk_md_user_id; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_favorite
    ADD CONSTRAINT md_favorite_user_id_107955df_fk_md_user_id FOREIGN KEY (user_id) REFERENCES public.md_user(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_ligand md_ligand_chain_id_a40d7778_fk_md_chain_id; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_ligand
    ADD CONSTRAINT md_ligand_chain_id_a40d7778_fk_md_chain_id FOREIGN KEY (chain_id) REFERENCES public.md_chain(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_ligand md_ligand_chain_same_simulation; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_ligand
    ADD CONSTRAINT md_ligand_chain_same_simulation FOREIGN KEY (simulation_id, chain_id) REFERENCES public.md_chain(simulation_id, id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_process_job md_process_job_ticket_id_590c1a63_fk_md_ticket_id; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_process_job
    ADD CONSTRAINT md_process_job_ticket_id_590c1a63_fk_md_ticket_id FOREIGN KEY (ticket_id) REFERENCES public.md_ticket(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_processed_file md_processed_file_file_type_fk; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_processed_file
    ADD CONSTRAINT md_processed_file_file_type_fk FOREIGN KEY (file_type) REFERENCES public.md_processed_file_type(name);


--
-- Name: md_replicate md_replicate_simulation_id_c03d294a_fk_md_simulation_id; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_replicate
    ADD CONSTRAINT md_replicate_simulation_id_c03d294a_fk_md_simulation_id FOREIGN KEY (simulation_id) REFERENCES public.md_simulation(id) ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_frontend_download_instance_processed_files md_repo_app_frontend_frontenddownloadinst_e0c92071_fk_md_repo_a; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_frontend_download_instance_processed_files
    ADD CONSTRAINT md_repo_app_frontend_frontenddownloadinst_e0c92071_fk_md_repo_a FOREIGN KEY (frontenddownloadinstance_id) REFERENCES public.md_frontend_download_instance(id) ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_frontend_download_instance_uploaded_files md_repo_app_frontend_frontenddownloadinst_f7653e25_fk_md_repo_a; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_frontend_download_instance_uploaded_files
    ADD CONSTRAINT md_repo_app_frontend_frontenddownloadinst_f7653e25_fk_md_repo_a FOREIGN KEY (frontenddownloadinstance_id) REFERENCES public.md_frontend_download_instance(id) ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_frontend_download_instance md_repo_app_frontend_simulation_id_2f1c68dd_fk_md_repo_a; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_frontend_download_instance
    ADD CONSTRAINT md_repo_app_frontend_simulation_id_2f1c68dd_fk_md_repo_a FOREIGN KEY (simulation_id) REFERENCES public.md_simulation(id) ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_frontend_download_instance_processed_files md_repo_app_frontend_simulationprocessedf_838f21b6_fk_md_repo_a; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_frontend_download_instance_processed_files
    ADD CONSTRAINT md_repo_app_frontend_simulationprocessedf_838f21b6_fk_md_repo_a FOREIGN KEY (simulationprocessedfile_id) REFERENCES public.md_processed_file(id) ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_frontend_download_instance_uploaded_files md_repo_app_frontend_simulationuploadedfi_ef83c0a1_fk_md_repo_a; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_frontend_download_instance_uploaded_files
    ADD CONSTRAINT md_repo_app_frontend_simulationuploadedfi_ef83c0a1_fk_md_repo_a FOREIGN KEY (simulationuploadedfile_id) REFERENCES public.md_uploaded_file(id) ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_frontend_download_instance md_repo_app_frontend_user_id_4e3be8c6_fk_md_repo_a; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_frontend_download_instance
    ADD CONSTRAINT md_repo_app_frontend_user_id_4e3be8c6_fk_md_repo_a FOREIGN KEY (user_id) REFERENCES public.md_user(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_ligand md_repo_app_ligand_simulation_id_5826262c_fk_md_repo_a; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_ligand
    ADD CONSTRAINT md_repo_app_ligand_simulation_id_5826262c_fk_md_repo_a FOREIGN KEY (simulation_id) REFERENCES public.md_simulation(id) ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_ticket md_repo_app_mdrepoti_created_by_id_5fa0ea1c_fk_md_repo_a; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_ticket
    ADD CONSTRAINT md_repo_app_mdrepoti_created_by_id_5fa0ea1c_fk_md_repo_a FOREIGN KEY (created_by_id) REFERENCES public.md_user(id) DEFERRABLE INITIALLY DEFERRED;


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
    ADD CONSTRAINT md_repo_app_simulati_simulation_id_0a34f055_fk_md_repo_a FOREIGN KEY (simulation_id) REFERENCES public.md_simulation(id) ON DELETE SET NULL DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_processed_file md_repo_app_simulati_simulation_id_0b584a48_fk_md_repo_a; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_processed_file
    ADD CONSTRAINT md_repo_app_simulati_simulation_id_0b584a48_fk_md_repo_a FOREIGN KEY (simulation_id) REFERENCES public.md_simulation(id) ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_simulation_uniprot md_repo_app_simulati_simulation_id_31f7f52c_fk_md_repo_a; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_simulation_uniprot
    ADD CONSTRAINT md_repo_app_simulati_simulation_id_31f7f52c_fk_md_repo_a FOREIGN KEY (simulation_id) REFERENCES public.md_simulation(id) ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_uploaded_file md_repo_app_simulati_simulation_id_7c2fdb70_fk_md_repo_a; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_uploaded_file
    ADD CONSTRAINT md_repo_app_simulati_simulation_id_7c2fdb70_fk_md_repo_a FOREIGN KEY (simulation_id) REFERENCES public.md_simulation(id) ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_simulation_pub md_repo_app_simulati_simulation_id_ccf7ab65_fk_md_repo_a; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_simulation_pub
    ADD CONSTRAINT md_repo_app_simulati_simulation_id_ccf7ab65_fk_md_repo_a FOREIGN KEY (simulation_id) REFERENCES public.md_simulation(id) ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_upload_instance_message md_repo_app_simulati_simulation_upload_id_419a4bf0_fk_md_repo_a; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_upload_instance_message
    ADD CONSTRAINT md_repo_app_simulati_simulation_upload_id_419a4bf0_fk_md_repo_a FOREIGN KEY (simulation_upload_id) REFERENCES public.md_upload_instance(id) ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED;


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
-- Name: md_simulation_collection md_simulation_collec_collection_id_c0931a52_fk_md_collec; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_simulation_collection
    ADD CONSTRAINT md_simulation_collec_collection_id_c0931a52_fk_md_collec FOREIGN KEY (collection_id) REFERENCES public.md_collection(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_simulation_collection md_simulation_collec_simulation_id_d01829fc_fk_md_simula; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_simulation_collection
    ADD CONSTRAINT md_simulation_collec_simulation_id_d01829fc_fk_md_simula FOREIGN KEY (simulation_id) REFERENCES public.md_simulation(id) ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_simulation md_simulation_contributor_id_70a8c834_fk_md_user_id; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_simulation
    ADD CONSTRAINT md_simulation_contributor_id_70a8c834_fk_md_user_id FOREIGN KEY (contributor_id) REFERENCES public.md_user(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_simulation_creator md_simulation_creato_simulation_id_372e91d4_fk_md_simula; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_simulation_creator
    ADD CONSTRAINT md_simulation_creato_simulation_id_372e91d4_fk_md_simula FOREIGN KEY (simulation_id) REFERENCES public.md_simulation(id) ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_simulation_creator md_simulation_creator_creator_id_12d0500b_fk_md_creator_id; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_simulation_creator
    ADD CONSTRAINT md_simulation_creator_creator_id_12d0500b_fk_md_creator_id FOREIGN KEY (creator_id) REFERENCES public.md_creator(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_solute md_solute_simulation_id_954a137d_fk_md_simulation_id; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_solute
    ADD CONSTRAINT md_solute_simulation_id_954a137d_fk_md_simulation_id FOREIGN KEY (simulation_id) REFERENCES public.md_simulation(id) ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_triage_delivery_finding md_triage_delivery_f_delivery_id_ca26c333_fk_md_triage; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_triage_delivery_finding
    ADD CONSTRAINT md_triage_delivery_f_delivery_id_ca26c333_fk_md_triage FOREIGN KEY (delivery_id) REFERENCES public.md_triage_delivery(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_triage_delivery_finding md_triage_delivery_f_finding_id_9cf41ab5_fk_md_triage; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_triage_delivery_finding
    ADD CONSTRAINT md_triage_delivery_f_finding_id_9cf41ab5_fk_md_triage FOREIGN KEY (finding_id) REFERENCES public.md_triage_finding(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_triage_delivery md_triage_delivery_parent_id_0ea2800a_fk_md_triage_delivery_id; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_triage_delivery
    ADD CONSTRAINT md_triage_delivery_parent_id_0ea2800a_fk_md_triage_delivery_id FOREIGN KEY (parent_id) REFERENCES public.md_triage_delivery(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_triage_delivery md_triage_delivery_ticket_id_f71e7ec6_fk_md_ticket_id; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_triage_delivery
    ADD CONSTRAINT md_triage_delivery_ticket_id_f71e7ec6_fk_md_ticket_id FOREIGN KEY (ticket_id) REFERENCES public.md_ticket(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_triage_finding md_triage_finding_process_job_id_424ed3a1_fk_md_process_job_id; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_triage_finding
    ADD CONSTRAINT md_triage_finding_process_job_id_424ed3a1_fk_md_process_job_id FOREIGN KEY (process_job_id) REFERENCES public.md_process_job(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_triage_finding md_triage_finding_ticket_id_13c8c123_fk_md_ticket_id; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_triage_finding
    ADD CONSTRAINT md_triage_finding_ticket_id_13c8c123_fk_md_ticket_id FOREIGN KEY (ticket_id) REFERENCES public.md_ticket(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_triage_finding md_triage_finding_upload_instance_id_c5f1bcf0_fk_md_upload; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_triage_finding
    ADD CONSTRAINT md_triage_finding_upload_instance_id_c5f1bcf0_fk_md_upload FOREIGN KEY (upload_instance_id) REFERENCES public.md_upload_instance(id) ON DELETE SET NULL DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_triage_occurrence md_triage_occurrence_finding_id_73e4a591_fk_md_triage; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_triage_occurrence
    ADD CONSTRAINT md_triage_occurrence_finding_id_73e4a591_fk_md_triage FOREIGN KEY (finding_id) REFERENCES public.md_triage_finding(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_triage_reaction md_triage_reaction_delivery_id_04ad9529_fk_md_triage; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_triage_reaction
    ADD CONSTRAINT md_triage_reaction_delivery_id_04ad9529_fk_md_triage FOREIGN KEY (delivery_id) REFERENCES public.md_triage_delivery(id) DEFERRABLE INITIALLY DEFERRED;


--
-- Name: md_uploaded_file md_uploaded_file_file_type_fk; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.md_uploaded_file
    ADD CONSTRAINT md_uploaded_file_file_type_fk FOREIGN KEY (file_type) REFERENCES public.md_uploaded_file_type(name);


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

\unrestrict Ho7gvdbAA10Td0iQRZ1AnnfhxbAiE6USBpT0vsTDha9yL0PrLo8p8f9hWy9kc3e


-- The two file-type lookups need their rows: every file row references one.

INSERT INTO public.md_processed_file_type (name) VALUES
    ('Full Trajectories (All)'),
    ('Minimal structure'),
    ('Minimal topology'),
    ('Minimal Trajectories (All)'),
    ('Minimal trajectory'),
    ('Preview image'),
    ('Processed structure'),
    ('Processed topology'),
    ('Processed trajectory'),
    ('Sampled minimal trajectory'),
    ('Sampled Trajectories (All)');

INSERT INTO public.md_uploaded_file_type (name) VALUES
    ('Checkpoint'),
    ('Input'),
    ('Logs'),
    ('Metadata'),
    ('Miscellaneous'),
    ('Parameters'),
    ('Periodic boundary condition'),
    ('Restart'),
    ('Structure'),
    ('Topology'),
    ('Trajectories (All)'),
    ('Trajectory'),
    ('User defined file');
