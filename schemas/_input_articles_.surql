DEFINE TABLE OVERWRITE _input_articles_ SCHEMAFULL;

DEFINE FIELD OVERWRITE text ON _input_articles_ TYPE string;
DEFINE FIELD OVERWRITE embeddings ON TABLE _input_articles_ TYPE array<float> ASSERT array::len($value) = 1024;

DEFINE INDEX OVERWRITE embeddings_vector_index ON _input_articles_
    FIELDS embeddings
    MTREE DIMENSION 1024
    DIST COSINE;