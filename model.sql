CREATE TABLE elector(
    elecId INTEGER PRIMARY KEY NOT NULL,
    elecIp TEXT NOT NULL UNIQUE
);

CREATE TABLE alternative(
    altId INTEGER PRIMARY KEY NOT NULL,
    altName TEXT UNIQUE,
    altDescription TEXT,
    altIcon TEXT
);

CREATE TABLE ranking(
    elecId INTEGER KEY NOT NULL,
    altId INTEGER KEY NOT NULL,
    rankMin INTEGER,
    rankMax INTEGER CHECK(rankMax >= rankMin)
);

CREATE TRIGGER removeAlternative BEFORE DELETE ON alternative
BEGIN
    DELETE FROM ranking WHERE ranking.altId = old.altId;
END;

CREATE TRIGGER removeElector BEFORE DELETE ON elector
BEGIN
    DELETE FROM ranking WHERE ranking.elecId = old.elecId;
END;