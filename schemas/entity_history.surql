DEFINE TABLE OVERWRITE entity_history SCHEMAFULL;

DEFINE FIELD OVERWRITE id ON entity_history TYPE array<record<entity> | datetime, 2>;

DEFINE FIELD OVERWRITE difference ON entity_history TYPE array<{
    op: string,
    path: string,
    value: any
}>;

DEFINE EVENT OVERWRITE track_data_changes ON entity 
    WHEN $event = "UPDATE"
        AND $after.data != $before.data 
    THEN {
        
        CREATE entity_history:[$after.id, time::now()] SET
            difference = value::diff($before.data, $after.data)
    };