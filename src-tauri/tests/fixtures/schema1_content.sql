INSERT INTO media_sources(id,source_type,root,label,kinds,availability) VALUES('source','local',x'2f6d65646961','Keep source','["audio"]','offline');
INSERT INTO media_items(id,source_id,relative_path,display_name,kind,size,mtime_ns,availability) VALUES('track','source',x'736f6e672e666c6163','Keep track','audio',42,'123','available');
INSERT INTO playlists VALUES('playlist','Keep playlist',10,20);
INSERT INTO playlist_items VALUES('playlist',0,'track'),('playlist',1,'track');
INSERT INTO resume_positions VALUES('track',1500,9000,25,0);
INSERT INTO settings VALUES('music_queue','{"entries":[{"entryId":"entry","mediaId":"track"}],"revision":7}',30),('unrelated','{"value":"preserve exactly"}',40);
