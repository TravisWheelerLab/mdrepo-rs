// Generated from the mdrepo PostgreSQL database.
// Regenerate with: diesel print-schema > src/schema.rs

diesel::table! {
    use diesel::sql_types::*;
    md_contribution (id) {
        id -> Int8,
        email -> Nullable<Varchar>,
        institution -> Nullable<Text>,
        name -> Nullable<Text>,
        orcid -> Nullable<Varchar>,
        simulation_id -> Nullable<Int8>,
        rank -> Int4,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    md_external_link (id) {
        id -> Int8,
        url -> Varchar,
        label -> Nullable<Varchar>,
        simulation_id -> Int8,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    md_feature_switch (id) {
        id -> Int8,
        irods_service_available -> Bool,
        simulation_animation_available -> Bool,
        media_service -> Varchar,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    md_frontend_download_instance (id) {
        id -> Int8,
        created_on -> Timestamptz,
        used -> Bool,
        simulation_id -> Int8,
        user_id -> Nullable<Int8>,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    md_frontend_download_instance_processed_files (id) {
        id -> Int8,
        frontenddownloadinstance_id -> Int8,
        simulationprocessedfile_id -> Int8,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    md_frontend_download_instance_uploaded_files (id) {
        id -> Int8,
        frontenddownloadinstance_id -> Int8,
        simulationuploadedfile_id -> Int8,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    md_ligand (id) {
        id -> Int8,
        name -> Text,
        smiles_string -> Text,
        simulation_id -> Int8,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    md_pdb (id) {
        id -> Int8,
        pdb_id -> Varchar,
        classification -> Nullable<Varchar>,
        title -> Nullable<Varchar>,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    md_processed_file (id) {
        id -> Int8,
        file_type -> Varchar,
        local_file_path -> Varchar,
        filename -> Varchar,
        simulation_id -> Int8,
        file_size_bytes -> Nullable<Int8>,
        description -> Nullable<Text>,
        md5_hash -> Nullable<Varchar>,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    md_pub (id) {
        id -> Int8,
        title -> Varchar,
        authors -> Varchar,
        journal -> Varchar,
        volume -> Int4,
        number -> Nullable<Varchar>,
        year -> Int4,
        pages -> Nullable<Varchar>,
        doi -> Varchar,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    md_simulation (id) {
        id -> Int8,
        description -> Nullable<Text>,
        run_commands -> Nullable<Text>,
        water_type -> Nullable<Varchar>,
        water_density -> Nullable<Float8>,
        duration -> Nullable<Float8>,
        sampling_frequency -> Nullable<Float8>,
        creation_date -> Timestamptz,
        software_id -> Nullable<Int8>,
        md_repo_ticket_id -> Nullable<Int8>,
        rmsd_values -> Nullable<Array<Float8>>,
        rmsf_values -> Nullable<Array<Float8>>,
        is_placeholder -> Bool,
        created_by_id -> Nullable<Int8>,
        replicate_group_id -> Nullable<Int8>,
        unique_file_hash_string -> Nullable<Text>,
        forcefield -> Nullable<Text>,
        forcefield_comments -> Nullable<Text>,
        temperature -> Nullable<Int4>,
        is_deprecated -> Bool,
        protonation_method -> Nullable<Text>,
        integration_timestep_fs -> Nullable<Int4>,
        short_description -> Nullable<Text>,
        pdb_id -> Nullable<Int8>,
        is_public -> Bool,
        fasta_sequence -> Nullable<Text>,
        user_accession -> Nullable<Text>,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    md_simulation_pub (id) {
        id -> Int8,
        simulation_id -> Int8,
        pub_id -> Int8,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    md_simulation_replicate_group (id) {
        id -> Int8,
        psf_hash -> Varchar,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    md_simulation_uniprot (id) {
        id -> Int8,
        simulation_id -> Int8,
        uniprot_id -> Int8,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    md_software (id) {
        id -> Int8,
        name -> Varchar,
        version -> Nullable<Varchar>,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    md_solvent (id) {
        id -> Int8,
        name -> Varchar,
        concentration -> Float8,
        simulation_id -> Int8,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    md_submission_completed_event (id) {
        id -> Int8,
        created_at -> Timestamptz,
        path -> Text,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    md_ticket (id) {
        id -> Int8,
        created_at -> Timestamptz,
        token -> Varchar,
        full_token -> Varchar,
        irods_tickets -> Nullable<Text>,
        guid -> Uuid,
        n_submissions -> Int4,
        created_by_id -> Int8,
        used_for_upload -> Bool,
        irods_creation_error -> Bool,
        ticket_type -> Varchar,
        no_files_found -> Bool,
        finished_generating -> Bool,
        orcid -> Nullable<Varchar>,
        upload_notification_sent -> Bool,
        processing_complete -> Bool,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    md_uniprot (id) {
        id -> Int8,
        uniprot_id -> Varchar,
        name -> Varchar,
        amino_length -> Int4,
        sequence -> Text,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    md_upload_instance (id) {
        id -> Int8,
        created_on -> Timestamptz,
        simulation_id -> Nullable<Int8>,
        user_id -> Nullable<Int8>,
        successful -> Nullable<Bool>,
        lead_contributor_orcid -> Varchar,
        filenames -> Nullable<Text>,
        ticket_id -> Nullable<Int8>,
        landing_id -> Nullable<Text>,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    md_upload_instance_message (id) {
        id -> Int8,
        timestamp -> Timestamptz,
        message -> Text,
        simulation_upload_id -> Int8,
        is_error -> Bool,
        is_warning -> Bool,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    md_uploaded_file (id) {
        id -> Int8,
        filename -> Varchar,
        file_type -> Varchar,
        simulation_id -> Int8,
        description -> Nullable<Varchar>,
        local_file_path -> Varchar,
        file_size_bytes -> Nullable<Int8>,
        md5_hash -> Nullable<Varchar>,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    md_user (id) {
        id -> Int8,
        password -> Varchar,
        last_login -> Nullable<Timestamptz>,
        is_superuser -> Bool,
        username -> Varchar,
        is_staff -> Bool,
        date_joined -> Timestamptz,
        first_name -> Varchar,
        last_name -> Varchar,
        registered -> Bool,
        email -> Varchar,
        institution -> Nullable<Varchar>,
        is_active -> Bool,
        can_contribute -> Bool,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    socialaccount_socialaccount (id) {
        id -> Int4,
        provider -> Varchar,
        uid -> Varchar,
        last_login -> Timestamptz,
        date_joined -> Timestamptz,
        extra_data -> Text,
        user_id -> Int8,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    socialaccount_socialapp (id) {
        id -> Int4,
        provider -> Varchar,
        name -> Varchar,
        client_id -> Varchar,
        secret -> Varchar,
        key -> Varchar,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    socialaccount_socialapp_sites (id) {
        id -> Int8,
        socialapp_id -> Int4,
        site_id -> Int4,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    socialaccount_socialtoken (id) {
        id -> Int4,
        token -> Text,
        token_secret -> Text,
        expires_at -> Nullable<Timestamptz>,
        account_id -> Int4,
        app_id -> Int4,
    }
}

diesel::allow_tables_to_appear_in_same_query!(
    md_contribution,
    md_external_link,
    md_feature_switch,
    md_frontend_download_instance,
    md_frontend_download_instance_processed_files,
    md_frontend_download_instance_uploaded_files,
    md_ligand,
    md_pdb,
    md_processed_file,
    md_pub,
    md_simulation,
    md_simulation_pub,
    md_simulation_replicate_group,
    md_simulation_uniprot,
    md_software,
    md_solvent,
    md_submission_completed_event,
    md_ticket,
    md_uniprot,
    md_upload_instance,
    md_upload_instance_message,
    md_uploaded_file,
    md_user,
    socialaccount_socialaccount,
    socialaccount_socialapp,
    socialaccount_socialapp_sites,
    socialaccount_socialtoken,
);
