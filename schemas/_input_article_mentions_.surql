DEFINE TABLE OVERWRITE _input_article_mentions_ SCHEMAFULL TYPE RELATION IN _input_articles_;

DEFINE FIELD OVERWRITE weight ON TABLE _input_article_mentions_ TYPE float;

# DEFINE FIELD OVERWRITE field ON _input_article_mentions_;