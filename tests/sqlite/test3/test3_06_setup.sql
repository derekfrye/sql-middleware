-- CREATE TABLE IF NOT EXISTS -- drop table test cascade
--                 test (
--                 recid INTEGER PRIMARY KEY AUTOINCREMENT
--                 , a int
--                 , b text
--                 , c datetime not null default current_timestamp
--                 , d real
--                 , e boolean
--                 , f blob
--                 , g json
--                 );

-- INSERT INTO test (a, b, c, d, e, f, g) VALUES
-- (1, 'Alpha', '2024-01-01 08:00:01', 10.5, 1, X'426C6F623132', '{"name": "Alice", "age": 30}');

-- INSERT INTO test (a, b, c, d, e, f, g) VALUES
-- (2, 'Bravo', '2024-01-02 09:15:00', 20.75, 0, X'426C6F623133', '{"name": "Bob", "age": 25}');

-- INSERT INTO test (a, b, c, d, e, f, g) VALUES
-- (3, 'Charlie', '2024-01-03 10:30:00', 30.25, 1, X'426C6F623134', '{"name": "Charlie", "age": 28}');

-- INSERT INTO test (a, b, c, d, e, f, g) VALUES
-- (4, 'Delta', '2024-01-04 11:45:00', 40.0, 0, X'426C6F623135', '{"name": "Diana", "age": 22}');

-- INSERT INTO test (a, b, c, d, e, f, g) VALUES
-- (5, 'Echo', '2024-01-05 13:00:00', 50.5, 1, X'426C6F623136', '{"name": "Evan", "age": 35}');

INSERT INTO test (a, b, c, d, e, f, g) VALUES
(?1, 'Foxtrot', '2024-01-06 14:15:00', 60.75, 0, X'426C6F623137', '{"name": "Fiona", "age": 27}');

-- INSERT INTO test (a, b, c, d, e, f, g) VALUES
-- (7, 'Golf', '2024-01-07 15:30:00', 70.25, 1, X'426C6F623138', '{"name": "George", "age": 32}');

-- INSERT INTO test (a, b, c, d, e, f, g) VALUES
-- (8, 'Hotel', '2024-01-08 16:45:00', 80.0, 0, X'426C6F623139', '{"name": "Hannah", "age": 24}');

-- INSERT INTO test (a, b, c, d, e, f, g) VALUES
-- (9, 'India', '2024-01-09 18:00:24', 90.5, 1, X'426C6F623130', '{"name": "Ian", "age": 29}');

-- INSERT INTO test (a, b, c, d, e, f, g) VALUES
-- (10, 'Juliet', '2024-01-10 19:15:23', 100.75, 0, X'426C6F623131', '{"name": "Julia", "age": 26}');


-- -- CREATE TABLE IF NOT EXISTS -- drop table event cascade
-- --                 test (
-- --                 a int
-- --                 , b text
-- --                 , c datetime not null default current_timestamp
-- --                 , d real
-- --                 , e boolean
-- --                 , f blob
-- --                 , g json
-- --                 )