
DEFINE EVENT OVERWRITE entity_changed ON entity 
    WHEN $event = "UPDATE"
        AND $after.data != $before.data 
    THEN {
        
        CREATE entity_history:[$after.id, time::now()] SET
            difference = value::diff($before.data, $after.data)
    };