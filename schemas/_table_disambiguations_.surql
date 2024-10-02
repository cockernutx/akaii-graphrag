DEFINE TABLE OVERWRITE _table_disambiguations_ SCHEMAFULL;

DEFINE FIELD OVERWRITE id ON _table_disambiguations_ TYPE string;
DEFINE FIELD OVERWRITE embeddings ON _table_disambiguations_ TYPE array<float> ASSERT array::len($value) = 1024;

DEFINE INDEX OVERWRITE embeddings_vector_index ON _table_disambiguations_
    FIELDS embeddings
    MTREE DIMENSION 1024
    DIST COSINE;