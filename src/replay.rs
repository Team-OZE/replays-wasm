use std::borrow::Cow;
use std::collections::HashMap;
use std::convert::TryInto;
use std::hint::black_box;
use std::io::{BufRead, BufReader, Cursor, Read, Seek, SeekFrom, Write};
use flate2::{Decompress, FlushDecompress};
use wasm_bindgen::prelude::wasm_bindgen;
use log::{info, warn};
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use serde::{Serialize, Serializer};
use web_sys::console::info;
use crate::replay::SlotRace::UNKNOWN;
use crate::utils;

#[derive(Serialize, FromPrimitive, Debug)]
enum SlotColor {
    RED = 1,
    BLUE = 2,
    TEAL = 3,
    PURPLE = 4,
    YELLOW = 5,
    ORANGE = 6,
    GREEN = 7,
    PINK = 8,
    GRAY = 9,
    LIGHTBLUE = 10,
    DARKGREEN = 11,
    BROWN = 12,
    MAROON = 13,
    NAVY = 14,
    TURQUOISE = 15,
    VIOLET = 16,
    WHEAT = 17,
    PEACH = 18,
    MINT = 19,
    LAVENDER = 20,
    COAL = 21,
    SNOW = 22,
    EMERALD = 23,
    PEANUT = 24,
    OBSERVER = 25,
    UNKNOWN = 127
}

#[derive(Serialize, FromPrimitive, Debug)]
enum SlotRace {
    HUMAN = 1,
    ORC = 2,
    NIGHTELF = 4,
    UNDEAD = 8,
    RANDOM = 20,
    FIXED = 40,
    UNKNOWN = 127
}

#[derive(Serialize, FromPrimitive, Debug)]
enum ComputerAIStrength {
    EASY = 0,
    NORMAL = 1,
    INSANE = 2,
    UNKNOWN = 127
}

#[derive(Serialize, FromPrimitive, Debug)]
enum SlotStatus {
    EMPTY = 0,
    CLOSED = 1,
    OCCUPIED = 2,
    UNKNOWN = 127
}
#[derive(Serialize)]
struct ReplayMeta {
    saving_player_id: u8,
    is_saving_player_host: bool,
    game_name: String,
    map_name: String,
    game_creator_battle_tag: String
}

#[derive(Serialize)]
struct GameSettings {
    game_speed: u8,
    vis_hide_terrain: bool,
    vis_map_explored: bool,
    vis_always_visible: bool,
    vis_default: bool,
    obs_mode: u8,
    teams_together: bool,
    fixed_teams: u8,
    shared_unit_control: bool,
    random_hero: bool,
    random_races: bool,
    obs_referees: bool
}

#[derive(Serialize)]
struct Slot {
    player_id: u8,
    map_download_percent: u8,
    status: SlotStatus,
    is_computer: bool,
    team_index: u8,
    color: SlotColor,
    race: SlotRace,
    ai_strength: ComputerAIStrength,
    hadnicap_percent: u8
}

#[derive(Serialize, Debug)]
struct ReplayPlayer {
    battle_tag: String
}

#[derive(Serialize)]
pub struct Replay {
    pub version: u8,
    metadata: ReplayMeta,
    game_settings: GameSettings,
    slots: Vec<Slot>,
    players: HashMap<u8, ReplayPlayer>
}

fn parse_dword(bytes: &[u8]) -> u32 {
    let mut data: u32 = 0;
    for j in (0u8..4u8) {
        data += 256u32.pow(j as u32) * bytes[j as usize] as u32
    }
    return data;
}

fn parse_word(bytes: &[u8]) -> u16 {
    let mut data: u16 = 0;
    for j in (0u8..2u8) {
        data += 256u16.pow(j as u32) * bytes[j as usize] as u16
    }
    return data;
}

fn cursor_read_dword<T>(cursor: &mut Cursor<T>) -> u32 where T: AsRef<[u8]> {
    let mut buf = [0u8; 4];
    cursor.read_exact(&mut buf).unwrap();
    return parse_dword(&buf);
}

fn cursor_read_word<T>(cursor: &mut Cursor<T>) -> u16 where T: AsRef<[u8]> {
    let mut buf = [0u8; 2];
    cursor.read_exact(&mut buf).unwrap();
    return parse_word(&buf);
}

fn cursor_read_nullterminated_string<T>(cursor: &mut Cursor<T>) -> String where T: AsRef<[u8]> {
    let mut string_buf: Vec<u8> = vec![];
    cursor.read_until(0x00, &mut string_buf).unwrap();
    let string = String::from_utf8_lossy(&string_buf[..string_buf.len()-1]);
    return string.to_string()
}

pub fn cursor_read_byte<T>(cursor: &mut Cursor<T>) -> u8 where T: AsRef<[u8]> {
    let mut buf: [u8;1] = [0u8];
    cursor.read_exact(&mut buf).unwrap();
    return buf[0];
}

fn cursor_skip_bytes<T>(cursor: &mut Cursor<T>, n: i64) where T: AsRef<[u8]> {
    cursor.seek(SeekFrom::Current(n)).unwrap();
}

fn decode_gamesettings(enc: &Vec<u8>) -> Vec<u8> {
    let mut i = 0;
    let mut mask: u8 = 0;
    let mut dec: Vec<u8> = vec![];
    while enc[i] != 0 {
        if i % 8 == 0 { mask = enc[i]; }
        else {
            if mask & (0x1 << (i%8)) == 0 {
                dec.push(enc[i] - 1)
            }
            else {
                dec.push(enc[i])
            }
        }
        i+=1;
    }
    return dec;
}

fn is_bit_set(byte: u8, i: u8) -> bool {
    return (byte & (1 << i)) != 0
}

fn get_bits_value(byte: u8, bits: &[u8]) -> u8 {
    let mut i: u8 = 0;
    let mut s: u8 = 0;
    while i < bits.len() as u8 {
        if is_bit_set(byte, bits[i as usize]) {
            s += 2_u8.pow(i as u32)
        }
        i+=1;
    }
    return s;
}

impl Replay {
    pub fn from_bytes(bytes: &[u8]) -> Replay {
        let mut reader = Cursor::new(bytes);
        info!("Total bytes length: {:?}", bytes.len());
        let mut header: [u8; 48] = [0; 48];
        reader.read_exact(&mut header).unwrap();
        info!("Replay version: {:?}", header);
        let version = header.get(0x0024).unwrap();
        let total_header_length = match version {
            0 => 64,
            1 => 68,
            _ => 68 // Unknown version - try 68
        };

        let mut subheader: Vec<u8> = vec![0; total_header_length - 48];
        reader.read_exact(&mut subheader).unwrap();

        let mut i: u32 = total_header_length as u32;
        let mut k = 0;
        let num_data_blocks = parse_dword(&header[44..48]);
        info!("Total data blocks: {:?}", num_data_blocks);
        let mut block_header: [u8; 12] = [0; 12];
        let mut data: Vec<u8> = vec![];

        while k < num_data_blocks {
            // 3.0 [Data block header]
            match reader.read_exact(&mut block_header) {
                Ok(_) => {
                    let block_data_length_bytes: &[u8] = block_header.get(0..4).unwrap();
                    let block_data_length_inflated_bytes: &[u8] = block_header.get(4..8).unwrap();
                    let block_data_length = parse_dword(block_data_length_bytes);
                    let block_data_length_inflated = parse_dword(block_data_length_inflated_bytes);

                    let crc_deflated = parse_word(block_header.get(8..10).unwrap());
                    let crc_inflated = parse_word(block_header.get(10..12).unwrap());
                    let mut decoder = Decompress::new(true);

                    info!("Word at offset {:#06x} ({:?}) {:?} ({:?}) / inflated: {:?} ({:?})", i, i, block_data_length_bytes, block_data_length, block_data_length_inflated_bytes, block_data_length_inflated);

                    let mut block_data: Vec<u8> = vec![0; block_data_length as usize];
                    match reader.read_exact(&mut block_data) {
                        Ok(_) => {
                            info!("Read datablock of length {:?}.", block_data_length);

                            let mut out: Vec<u8> = Vec::with_capacity(block_data_length_inflated as usize);

                            // 4.0 [Decompressed data]
                            decoder.decompress_vec(&block_data, &mut out, FlushDecompress::Sync).unwrap();
                            decoder.reset(true);
                            info!("Decompressed block length: {:?} / begins with {:?}", out.len(), out.get(0..8).unwrap());

                            data.append(&mut out);
                        }
                        Err(_) => {
                            warn!("Failed to read datablock of length {:?}.", block_data_length);
                        }
                    };
                    i += block_data_length + 12;
                    k+=1;
                }
                Err(_) => break
            }
        }


        info!("Finished replay decoding. Total decoded data length: {:?}", data.len());
        info!("Data starts with {:?}", data.get(0..128).unwrap());

        // Decoding of the actual data

        let mut cursor = Cursor::new(&data);


        // 4.1 [PlayerRecord]
        let player_is_host = cursor_read_byte(&mut cursor) == 0x00;
        let player_id = cursor_read_byte(&mut cursor);

        // Something new - undocumented
        cursor_skip_bytes(&mut cursor, 4);

        let player_name = cursor_read_nullterminated_string(&mut cursor);
        info!("Player name: {:?}", player_name);

        let additional_data_size_byte = cursor_read_byte(&mut cursor);
        cursor_skip_bytes(&mut cursor, additional_data_size_byte as i64);


        // 4.2 [GameName]
        let game_name = cursor_read_nullterminated_string(&mut cursor);
        info!("Game name: {:?}", game_name);

        // There seems to be an additional NUL byte
        cursor_skip_bytes(&mut cursor, 1);

        // 4.3 [Encoded String]
        let mut encoded_gamesettings_buf: Vec<u8> = vec![];
        cursor.read_until(0x00, &mut encoded_gamesettings_buf).unwrap();

        let game_settings_buf = decode_gamesettings(&encoded_gamesettings_buf);
        info!("Decoded gamesettings: {:?}", game_settings_buf);

        // 4.4 [GameSettings]
        let game_speed = get_bits_value(game_settings_buf[0], [0, 1].as_ref());
        let vis_hide_terrain = get_bits_value(game_settings_buf[1], [0].as_ref()) == 1;
        let vis_map_explored = get_bits_value(game_settings_buf[1], [1].as_ref()) == 1;
        let vis_always_visible = get_bits_value(game_settings_buf[1], [2].as_ref()) == 1;
        let vis_default = get_bits_value(game_settings_buf[1], [3].as_ref()) == 1;
        let obs_mode = get_bits_value(game_settings_buf[1], [4, 5].as_ref());
        let teams_together = get_bits_value(game_settings_buf[1], [6].as_ref()) == 1;

        let fixed_teams = get_bits_value(game_settings_buf[2], [1,2].as_ref());
        let shared_unit_control = get_bits_value(game_settings_buf[3], [0].as_ref()) == 1;
        let random_hero = get_bits_value(game_settings_buf[3], [1].as_ref()) == 1;
        let random_races = get_bits_value(game_settings_buf[3], [2].as_ref()) == 1;
        let obs_referees = get_bits_value(game_settings_buf[3], [6].as_ref()) == 1;

        // 4.5 [Map&CreatorName]
        let mut subcursor = Cursor::new(game_settings_buf[13..].as_ref());
        let map_name = cursor_read_nullterminated_string(&mut subcursor);
        let game_creator_name = cursor_read_nullterminated_string(&mut subcursor);

        // 4.6 [PlayerCount]
        let num_players_slots = cursor_read_dword(&mut cursor);

        // 4.7 [GameType]
        let game_type = cursor_read_byte(&mut cursor);
        let is_private_custom_game = cursor_read_byte(&mut cursor);
        cursor_skip_bytes(&mut cursor, 2);

        // 4.8 [LanguageID?]
        cursor_skip_bytes(&mut cursor, 4);

        // 4.9 [PlayerList]
        let mut player_list:HashMap<u8, ReplayPlayer> = HashMap::new();
        player_list.insert(player_id, ReplayPlayer { battle_tag: player_name.clone() });
        let mut next_record_id = cursor_read_byte(&mut cursor);
        while next_record_id == 0x00 || next_record_id == 0x16 {
            let cur_player_id = cursor_read_byte(&mut cursor);
            // cursor_skip_bytes(&mut cursor, 4);;
            let cur_player_name = cursor_read_nullterminated_string(&mut cursor);
            let additional_data_size_byte = cursor_read_byte(&mut cursor);
            cursor_skip_bytes(&mut cursor, additional_data_size_byte as i64);
            player_list.insert(cur_player_id, ReplayPlayer { battle_tag: cur_player_name });
            next_record_id = cursor_read_byte(&mut cursor);
        }
        info!("Loaded player list: {:?}", player_list);

        // Reforged player metadata
        while next_record_id == 0x39 {
            let cur_record_subtype = cursor_read_byte(&mut cursor);
            let cur_record_data_length = cursor_read_dword(&mut cursor);

            cursor_skip_bytes(&mut cursor, cur_record_data_length as i64);
            // TODO: Maybe parse this data too

            next_record_id = cursor_read_byte(&mut cursor);
        }

        // 4.10 [GameStartRecord]
        if next_record_id != 0x19 {
            let mut buf = [0u8; 128];
            cursor.read_exact(&mut buf).unwrap();
            panic!("GameStartRecord did not follow PlayerList: next record id = {:?}. Following bytes: {:?}", next_record_id, buf)
        }

        let data_length = cursor_read_word(&mut cursor);
        let count_slotrecords = cursor_read_byte(&mut cursor);
        let mut i = 0u8;

        let mut slots: Vec<Slot> = Vec::with_capacity(count_slotrecords as usize);

        while i < count_slotrecords {
            let cur_slot_player_id = cursor_read_byte(&mut cursor);
            let cur_slot_map_download_percent = cursor_read_byte(&mut cursor);
            let status_byte = cursor_read_byte(&mut cursor);
            let cur_slot_status = SlotStatus::from_u8(status_byte)
                .or(Option::from(SlotStatus::UNKNOWN))
                .unwrap();
            let cur_slot_is_computer_player = cursor_read_byte(&mut cursor) == 1;
            let cur_slot_team_index = cursor_read_byte(&mut cursor);
            let color_byte = cursor_read_byte(&mut cursor);
            let cur_slot_color =
                SlotColor::from_u8(color_byte + 1)
                    .or(Option::from(SlotColor::UNKNOWN))
                    .unwrap();
            let race_byte = cursor_read_byte(&mut cursor);
            let cur_slot_player_race =
                SlotRace::from_u8(race_byte)
                    .or(Option::from(UNKNOWN))
                    .unwrap();
            let cur_slot_player_computer_ai_strenth =
                ComputerAIStrength::from_u8(cursor_read_byte(&mut cursor))
                    .or(Option::from(ComputerAIStrength::UNKNOWN))
                    .unwrap();
            let cur_slot_handicap_percent = cursor_read_byte(&mut cursor);

            info!("Player slot record read: pid = {:?} status = {:?} is_comp = {:?} team = {:?} color = {:?} ({:?}) race = {:?} ({:?})",
                cur_slot_player_id, cur_slot_status, cur_slot_is_computer_player, cur_slot_team_index, cur_slot_color, color_byte, cur_slot_player_race, race_byte);

            slots.push(Slot {
                player_id: cur_slot_player_id,
                map_download_percent: cur_slot_map_download_percent,
                status: cur_slot_status,
                is_computer: cur_slot_is_computer_player,
                team_index: cur_slot_team_index,
                color: cur_slot_color,
                race: cur_slot_player_race,
                ai_strength: cur_slot_player_computer_ai_strenth,
                hadnicap_percent: cur_slot_handicap_percent,
            });

            i+=1;
        }

        let random_seed = cursor_read_dword(&mut cursor);
        info!("Random seed: {:?}", random_seed);
        let selection_mode = cursor_read_byte(&mut cursor);
        info!("Selection mode: {:?}", selection_mode);
        let start_spot_count = cursor_read_byte(&mut cursor);
        info!("Start spots count: {:?}", start_spot_count);

        return Replay {
            version: *version,
            metadata: ReplayMeta {
                game_name,
                is_saving_player_host: player_is_host,
                saving_player_id: player_id,
                map_name,
                game_creator_battle_tag: game_creator_name
            },
            game_settings: GameSettings {
                fixed_teams,
                shared_unit_control,
                random_hero,
                random_races,
                obs_referees,
                vis_default,
                vis_hide_terrain,
                vis_always_visible,
                vis_map_explored,
                teams_together,
                obs_mode,
                game_speed
            },
            slots,
            players: player_list
        };
    }
}