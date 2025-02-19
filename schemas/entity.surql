DEFINE TABLE OVERWRITE entity SCHEMAFULL;

DEFINE FIELD OVERWRITE id ON entity TYPE string;

DEFINE FIELD OVERWRITE data ON entity FLEXIBLE TYPE object DEFAULT {};

DEFINE FIELD OVERWRITE creation_date ON entity TYPE datetime DEFAULT time::now() READONLY;


DEFINE FIELD OVERWRITE title_embeddings ON TABLE entity TYPE array<float> ASSERT array::len($value) = 1024;
DEFINE INDEX OVERWRITE embeddings_vector_index ON entity
    FIELDS title_embeddings
    HNSW DIMENSION 1024;