DEFINE TABLE OVERWRITE entity_history SCHEMAFULL;

DEFINE FIELD OVERWRITE id ON entity_history TYPE array<record<entity> | datetime, 2>;

DEFINE FIELD OVERWRITE difference ON entity_history TYPE array<{
    op: string,
    path: string,
    value: any
}>;

