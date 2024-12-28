insert into event (name, espn_id) values ('PGA Championship', 401580351);
-- or https://site.web.api.espn.com/apis/site/v2/sports/golf/pga/leaderboard/players?region=us&lang=en&event=401580351
-- player specific: https://site.web.api.espn.com/apis/site/v2/sports/golf/pga/leaderboard/401580360/playersummary?season=2024&player=3470
insert into player (name, espn_id) values ('Davis Thompson',4602218);
insert into player (name, espn_id) values ('Peter Malnati',5692);
insert into player (name, espn_id) values ('Stephan Jaeger',6937);

insert into player (name, espn_id) values ('Rory McIlroy', 3470);
insert into player (name, espn_id) values ('Viktor Hovland', 4364873);
insert into player (name, espn_id) values ('Scottie Scheffler', 9478);
insert into player (name, espn_id) values ('Xander Schauffele', 10140);
insert into player (name, espn_id) values ('Jordan Spieth', 5467);

insert into player (name, espn_id) values ('Collin Morikawa', 10592);
insert into player (name, espn_id) values ('Hideki Matsuyama', 5860);
insert into player (name, espn_id) values ('Ludvig Åberg', 4375972);
insert into player (name, espn_id) values ('Wyndham Clark', 11119);
insert into player (name, espn_id) values ('Matt Fitzpatrick', 9037);

insert into player (name, espn_id) values ('Jason Day', 1680);
insert into player (name, espn_id) values ('Bryson DeChambeau', 10046);
insert into player (name, espn_id) values ('Jon Rahm', 9780);
insert into player (name, espn_id) values ('Max Homa', 8973);
insert into player (name, espn_id) values ('Brooks Koepka', 6798);

insert into player (name, espn_id) values ('Justin Thomas', 4848);
insert into player (name, espn_id) values ('Sahith Theegala', 10980);
insert into player (name, espn_id) values ('Russell Henley', 5409);
insert into player (name, espn_id) values ('Cameron Smith', 9131);

insert into golfuser (name) values ('Player1');
insert into golfuser (name) values ('Player2');
insert into golfuser (name) values ('Player3');
insert into golfuser (name) values ('Player4');
insert into golfuser (name) values ('Player5');

-- begin transaction;

insert into event_user_player (event_id, user_id, player_id)select (select event_id from event where espn_id = 401580351), (select user_id from golfuser where name = 'Player1'), (select player_id from player where espn_id = 3470);
insert into event_user_player (event_id, user_id, player_id)select (select event_id from event where espn_id = 401580351), (select user_id from golfuser where name = 'Player2'), (select player_id from player where espn_id = 4364873);
insert into event_user_player (event_id, user_id, player_id)select (select event_id from event where espn_id = 401580351), (select user_id from golfuser where name = 'Player3'), (select player_id from player where espn_id = 9478);
insert into event_user_player (event_id, user_id, player_id)select (select event_id from event where espn_id = 401580351), (select user_id from golfuser where name = 'Player4'), (select player_id from player where espn_id = 10140);
insert into event_user_player (event_id, user_id, player_id)select (select event_id from event where espn_id = 401580351), (select user_id from golfuser where name = 'Player5'), (select player_id from player where espn_id = 5467);

insert into event_user_player (event_id, user_id, player_id)select (select event_id from event where espn_id = 401580351), (select user_id from golfuser where name = 'Player1'), (select player_id from player where espn_id = 10592);
insert into event_user_player (event_id, user_id, player_id)select (select event_id from event where espn_id = 401580351), (select user_id from golfuser where name = 'Player2'), (select player_id from player where espn_id = 5860);
insert into event_user_player (event_id, user_id, player_id)select (select event_id from event where espn_id = 401580351), (select user_id from golfuser where name = 'Player3'), (select player_id from player where espn_id = 4375972);
insert into event_user_player (event_id, user_id, player_id)select (select event_id from event where espn_id = 401580351), (select user_id from golfuser where name = 'Player4'), (select player_id from player where espn_id = 11119);
insert into event_user_player (event_id, user_id, player_id)select (select event_id from event where espn_id = 401580351), (select user_id from golfuser where name = 'Player5'), (select player_id from player where espn_id = 9037);

insert into event_user_player (event_id, user_id, player_id)select (select event_id from event where espn_id = 401580351), (select user_id from golfuser where name = 'Player1'), (select player_id from player where espn_id = 1680);
insert into event_user_player (event_id, user_id, player_id)select (select event_id from event where espn_id = 401580351), (select user_id from golfuser where name = 'Player2'), (select player_id from player where espn_id = 10046);
insert into event_user_player (event_id, user_id, player_id)select (select event_id from event where espn_id = 401580351), (select user_id from golfuser where name = 'Player3'), (select player_id from player where espn_id = 9780);
insert into event_user_player (event_id, user_id, player_id)select (select event_id from event where espn_id = 401580351), (select user_id from golfuser where name = 'Player4'), (select player_id from player where espn_id = 8973);
insert into event_user_player (event_id, user_id, player_id)select (select event_id from event where espn_id = 401580351), (select user_id from golfuser where name = 'Player5'), (select player_id from player where espn_id = 6798);

-- commit;