DEFINE TABLE OVERWRITE input_article SCHEMAFULL;

DEFINE FIELD OVERWRITE text ON input_article TYPE string;

DEFINE FIELD OVERWRITE metadata ON input_article FLEXIBLE TYPE object;

DEFINE FIELD OVERWRITE creation_date ON input_article TYPE datetime DEFAULT time::now() READONLY;

DEFINE FIELD OVERWRITE mentions_entity ON input_article TYPE set<record<entity>> DEFAULT [];

DEFINE FIELD OVERWRITE chunks ON TABLE input_article 
    TYPE array<record<input_article_chunk>> 
    VALUE array::distinct(
        SELECT VALUE id FROM input_article_chunk WHERE article = $parent.id
    ) DEFAULT [];