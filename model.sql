PRAGMA foreign_keys = ON;

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
    elecId INTEGER NOT NULL REFERENCES elector(elecId) ON DELETE CASCADE,
    altId INTEGER NOT NULL REFERENCES alternative(altId) ON DELETE CASCADE,
    rankMin INTEGER,
    rankMax INTEGER CHECK(rankMax >= rankMin),
    PRIMARY KEY(elecId, altId)
);
