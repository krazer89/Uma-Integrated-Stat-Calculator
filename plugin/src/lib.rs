#![feature(mpmc_channel)]

pub mod il2cpp;
pub mod plugin_api;

use std::{
    collections::HashMap,
    ffi::{c_char, c_void},
    net::{TcpListener, TcpStream},
    ptr::{null, null_mut},
    sync::{
        LazyLock,
        mpmc::{Receiver, Sender, channel},
    },
    thread,
};

use bytes::Buf;
use il2cpp::types::*;
use int_enum::IntEnum;
use plugin_api::{InitResult, Vtable};
use serde::Serialize;
use tungstenite::{
    Bytes,
    Message::{Binary, Text},
    Utf8Bytes, accept,
};

use crate::{
    il2cpp::{cute::ui::AtlasReference, gallop::helper::*, helper::*},
    plugin_api::VERSION,
};

type DialogTrainedCharacterDetailCreateSetupParameter =
    unsafe extern "C" fn(*mut Il2CppObject, *mut c_char, *mut c_void, *mut MethodInfo,);
type OnClickListItem = unsafe extern "C" fn(*mut Il2CppObject, *mut Il2CppObject, *mut MethodInfo);

static mut VTABLE: Option<&'static Vtable> = None;
static TXRX: LazyLock<(Sender<String>, Receiver<String>)> = LazyLock::new(|| channel());
static TX: LazyLock<Sender<String>> = LazyLock::new(|| TXRX.0.clone());
static RX: LazyLock<Receiver<String>> = LazyLock::new(|| TXRX.1.clone());

#[derive(Default, Serialize, IntEnum)]
#[repr(C)]
enum SkillTag {
    #[default]
    RunningStyleBegin = 100,
    Nige = 101,
    Senko = 102,
    Sashi = 103,
    Oikomi = 104,
    RunningStyleEnd = 199,
    DistanceBegin = 200,
    Short = 201,
    Mile = 202,
    Middle = 203,
    Long = 204,
    DistanceEnd = 299,
    SPEED = 401,
    STAMINA = 402,
    POWER = 403,
    GUTS = 404,
    WIZ = 405,
    DOWN = 406,
    SPECIAL = 407,
    GroundBegin = 500,
    Turf = 501,
    Dirt = 502,
    GroundEnd = 599,
    ScenarioBegin = 800,
    ScenarioEnd = 899,
}

#[derive(Default, Serialize)]
struct SkillData {
    id: i32,
    name: String,
    group_id: i32,
    rarity: i32,
    level: i32,
    remark: String,
    skill_tags: Vec<SkillTag>,
    grade_value: i32,
    is_level_up: bool,
    is_unique_skill: bool,
}

#[derive(Default, Serialize)]
struct CharacterData {
    id: i32,
    name: String,
    rank_score: i32,
    speed: i32,
    stamina: i32,
    power: i32,
    guts: i32,
    wiz: i32,
    proper_ground_turf: i32,
    proper_ground_dirt: i32,
    proper_distance_short: i32,
    proper_distance_mile: i32,
    proper_distance_middle: i32,
    proper_distance_long: i32,
    proper_running_style_nige: i32,
    proper_running_style_senko: i32,
    proper_running_style_sashi: i32,
    proper_running_style_oikomi: i32,
    acquired_skills: Vec<SkillData>,
}

#[derive(Serialize)]
enum MessageType {
    CharacterUpdate,
    SkillPlus,
    SkillMinus,
}

#[derive(Serialize)]
enum MessageData {
    CharacterUpdate(CharacterData),
    SkillUpdate(Vec<SkillData>),
}

#[derive(Serialize)]
struct Message {
    message_type: MessageType,
    message: MessageData,
}

// struct StaticResources {
//     rank_atlas: Vec<u8>,
//     rank_icons: Vec<u8>,
// }

unsafe fn get_hachimi_and_interceptor() -> (*const c_void, *const c_void) {
    unsafe {
        let vtable = VTABLE.unwrap();
        let hachimi = (vtable.hachimi_instance)();
        let interceptor = (vtable.hachimi_get_interceptor)(hachimi);
        (hachimi, interceptor)
    }
}

fn skilldata_to_struct(skill_data: *mut Il2CppObject) -> SkillData {
    unsafe {
        let vtable = VTABLE.unwrap();

        let skill_data_class = get_class("Gallop.MasterSkillData.SkillData");

        let get_enum_tag_list: GetPointer =
            std::mem::transmute(get_method(skill_data_class, "GetEnumTagList", 0));
        let is_unique_skill: GetU8 =
            std::mem::transmute(get_method(skill_data_class, "IsUniqueSkill", 0));

        let name = get_object(skill_data_class, "Name", skill_data) as *const Il2CppString;
        let name_string = il2cppstring_as_string(name.as_ref_unchecked());
        let remark = get_object(skill_data_class, "Remarks", skill_data) as *const Il2CppString;
        let remark_string = il2cppstring_as_string(remark.as_ref_unchecked());

        let skill_tag_list = get_enum_tag_list(skill_data, null()) as *mut Il2CppObject;
        let list_class = *(skill_tag_list as *mut *mut Il2CppClass);
        let item_field = (vtable.il2cpp_get_field_from_name)(list_class, c"_items".as_ptr());
        let size = get_i32_field(list_class, "_size", skill_tag_list);
        let mut skill_tag_array: *mut Il2CppArray = null_mut();
        (vtable.il2cpp_get_field_value)(
            skill_tag_list,
            item_field,
            &mut skill_tag_array as *mut _ as _,
        );
        let mut skill_tag_vec: Vec<SkillTag> = Vec::new();
        for j in 0..size as usize {
            let skill_tag_int = array_get_int(skill_tag_array.as_ref_unchecked(), j);
            if let Ok(skill_tag) = SkillTag::try_from(skill_tag_int as isize) {
                skill_tag_vec.push(skill_tag);
            }
        }

        let getter_i32_field =
            |field: &str| -> i32 { return get_i32_field(skill_data_class, field, skill_data) };

        let id = getter_i32_field("Id");
        return SkillData {
            id: id,
            name: name_string,
            group_id: id / 10,
            rarity: getter_i32_field("Rarity"),
            level: 0,
            remark: remark_string,
            skill_tags: skill_tag_vec,
            grade_value: getter_i32_field("GradeValue"),
            is_level_up: get_u8(skill_data_class, "IsLevelUp", skill_data) != 0,
            is_unique_skill: is_unique_skill(skill_data, null()) != 0,
        };
    }
}

fn acquiredskill_to_struct(acquired_skill: *mut Il2CppObject) -> SkillData {
    let acquired_skill_class = get_class("Gallop.WorkSkillData.AcquiredSkill");

    let level = get_i32(acquired_skill_class, "Level", acquired_skill);
    let skill_data = get_object(acquired_skill_class, "MasterData", acquired_skill);
    let mut output = skilldata_to_struct(skill_data);
    output.level = level;

    return output;
}

fn worksinglemodecharadata_to_struct(
    work_single_mode_chara_data: *mut Il2CppObject,
) -> CharacterData {
    unsafe {
        let vtable = VTABLE.unwrap();

        let work_single_mode_chara_data_class = get_gallop_class("WorkSingleModeCharaData");
        let card_data_class = get_class("Gallop.MasterCardData.CardData");

        let getter_i32 = |property: &str| -> i32 {
            return get_i32(
                work_single_mode_chara_data_class,
                property,
                work_single_mode_chara_data,
            );
        };

        let skill_list = get_object(
            work_single_mode_chara_data_class,
            "AcquiredSkillList",
            work_single_mode_chara_data,
        );

        let list_class = *(skill_list as *mut *mut Il2CppClass);
        let item_field = (vtable.il2cpp_get_field_from_name)(list_class, c"_items".as_ptr());
        let size = get_i32_field(list_class, "_size", skill_list);
        let mut skill_array: *mut Il2CppArray = null_mut();
        (vtable.il2cpp_get_field_value)(skill_list, item_field, &mut skill_array as *mut _ as _);
        let mut skill_vec: Vec<SkillData> = Vec::new();
        for i in 0..size as usize {
            let skill = array_get_obj(skill_array.as_ref_unchecked(), i);
            skill_vec.push(acquiredskill_to_struct(skill));
        }

        let card_data = get_object(
            work_single_mode_chara_data_class,
            "CardData",
            work_single_mode_chara_data,
        );
        let name = get_pointer(card_data_class, "Charaname", card_data) as *mut Il2CppString;

        return CharacterData {
            id: getter_i32("CharaId"),
            name: il2cppstring_as_string(name.as_ref_unchecked()),
            rank_score: 0,
            speed: getter_i32("Speed"),
            stamina: getter_i32("Stamina"),
            power: getter_i32("Power"),
            guts: getter_i32("Guts"),
            wiz: getter_i32("Wiz"),
            proper_ground_turf: getter_i32("ProperGroundTurf"),
            proper_ground_dirt: getter_i32("ProperGroundDirt"),
            proper_distance_short: getter_i32("ProperDistanceShort"),
            proper_distance_mile: getter_i32("ProperDistanceMile"),
            proper_distance_middle: getter_i32("ProperDistanceMiddle"),
            proper_distance_long: getter_i32("ProperDistanceLong"),
            proper_running_style_nige: getter_i32("ProperRunningStyleNige"),
            proper_running_style_senko: getter_i32("ProperRunningStyleSenko"),
            proper_running_style_sashi: getter_i32("ProperRunningStyleSashi"),
            proper_running_style_oikomi: getter_i32("ProperRunningStyleOikomi"),
            acquired_skills: skill_vec,
        };
    }
}

fn trainedcharadata_to_struct(trained_chara_data: *mut Il2CppObject) -> CharacterData {
    unsafe {
        let trained_chara_data_class = get_class("Gallop.WorkTrainedCharaData.TrainedCharaData");
        let chara_data_class = get_class("Gallop.MasterCharaData.CharaData");
        // let card_rarity_data_class = get_class("Gallop.MasterCardRarityData.CardRarityData");

        let getter_i32 = |property: &str| -> i32 {
            return get_i32(trained_chara_data_class, property, trained_chara_data);
        };

        let skill_list_il2cpp = get_pointer(
            trained_chara_data_class,
            "AcquiredSkillArray",
            trained_chara_data,
        ) as *const Il2CppArray;

        let mut skill_vec: Vec<SkillData> = Vec::new();
        for i in 0..(*skill_list_il2cpp).max_length {
            let skill = array_get_obj(skill_list_il2cpp.as_ref_unchecked(), i);

            skill_vec.push(acquiredskill_to_struct(skill));
        }

        let master_chara_data = get_object(
            trained_chara_data_class,
            "MasterCharaData",
            trained_chara_data,
        );

        let name =
            get_object(trained_chara_data_class, "Name", trained_chara_data) as *const Il2CppString;
        let name_string = il2cppstring_as_string(name.as_ref_unchecked());
        let rank_score = getter_i32("RankScore");

        // let card_rarity_data = get_object(
        //     trained_chara_data_class,
        //     "MasterCardRarityData",
        //     trained_chara_data,
        // );
        // let card_id = get_i32_field(card_rarity_data_class, "CardId", card_rarity_data);
        // let character_button = CharacterButton::new(card_id, 5, 5, -1, get_total_rank(rank_score));
        // let portrait = BASE64_STANDARD.encode(character_button.get_portrait());

        return CharacterData {
            id: get_i32_field(chara_data_class, "Id", master_chara_data),
            name: name_string,
            rank_score: rank_score,
            speed: getter_i32("Speed"),
            stamina: getter_i32("Stamina"),
            power: getter_i32("Power"),
            guts: getter_i32("Guts"),
            wiz: getter_i32("Wiz"),
            proper_ground_turf: getter_i32("ProperGroundTurf"),
            proper_ground_dirt: getter_i32("ProperGroundDirt"),
            proper_distance_short: getter_i32("ProperDistanceShort"),
            proper_distance_mile: getter_i32("ProperDistanceMile"),
            proper_distance_middle: getter_i32("ProperDistanceMiddle"),
            proper_distance_long: getter_i32("ProperDistanceLong"),
            proper_running_style_nige: getter_i32("ProperRunningStyleNige"),
            proper_running_style_senko: getter_i32("ProperRunningStyleSenko"),
            proper_running_style_sashi: getter_i32("ProperRunningStyleSashi"),
            proper_running_style_oikomi: getter_i32("ProperRunningStyleOikomi"),
            acquired_skills: skill_vec,
        };
    }
}

unsafe extern "C" fn dialog_trained_character_detail_create_setup_parameter_hook(
    trained_chara_data: *mut Il2CppObject,
    trainer_name: *mut c_char,
    on_change_partner: *mut c_void,
    method_info: *mut MethodInfo,
) {
    unsafe {
        let vtable = VTABLE.unwrap();
        let tx = TX.clone();

        let (_, interceptor) = get_hachimi_and_interceptor();
        let trampoline = (vtable.interceptor_get_trampoline_addr)(
            interceptor,
            dialog_trained_character_detail_create_setup_parameter_hook as *mut c_void,
        );
        let original: DialogTrainedCharacterDetailCreateSetupParameter =
            std::mem::transmute(trampoline);

        let character_data = trainedcharadata_to_struct(trained_chara_data);
        let message = Message {
            message_type: MessageType::CharacterUpdate,
            message: MessageData::CharacterUpdate(character_data),
        };
        tx.send(serde_json::to_string(&message).unwrap()).unwrap();

        drop(message);
        drop(tx);

        return original(
            trained_chara_data,
            trainer_name,
            on_change_partner,
            method_info,
        );
    }
}

unsafe extern "C" fn parts_single_mode_character_status_panel_setup_hook(
    this: *mut Il2CppObject,
    chara_data: *mut Il2CppObject,
    method_info: *mut MethodInfo,
) {
    unsafe {
        let vtable = VTABLE.unwrap();
        let tx = TX.clone();

        let (_, interceptor) = get_hachimi_and_interceptor();
        let trampoline = (vtable.interceptor_get_trampoline_addr)(
            interceptor,
            parts_single_mode_character_status_panel_setup_hook as *mut c_void,
        );
        let original: OnClickListItem = std::mem::transmute(trampoline);

        let character_data = worksinglemodecharadata_to_struct(chara_data);
        let message = Message {
            message_type: MessageType::CharacterUpdate,
            message: MessageData::CharacterUpdate(character_data),
        };
        tx.send(serde_json::to_string(&message).unwrap()).unwrap();

        drop(message);
        drop(tx);

        return original(this, chara_data, method_info);
    }
}

fn single_mode_main_view_training_status_setup_hook(
    this: *mut Il2CppObject,
    chara_data: *mut Il2CppObject,
) {
    unsafe {
        let vtable = VTABLE.unwrap();
        let tx = TX.clone();

        let (_, interceptor) = get_hachimi_and_interceptor();
        let trampoline = (vtable.interceptor_get_trampoline_addr)(
            interceptor,
            single_mode_main_view_training_status_setup_hook as *mut c_void,
        );
        let original: unsafe extern "C" fn(*mut Il2CppObject, *mut Il2CppObject) =
            std::mem::transmute(trampoline);

        let character_data = worksinglemodecharadata_to_struct(chara_data);
        let message = Message {
            message_type: MessageType::CharacterUpdate,
            message: MessageData::CharacterUpdate(character_data),
        };
        tx.send(serde_json::to_string(&message).unwrap()).unwrap();

        return original(this, chara_data);
    }
}

fn on_click_list_item_common(item: *mut Il2CppObject, plus: bool) -> Vec<SkillData> {
    unsafe {
        let parts_single_mode_skill_learning_list_item_class =
            get_gallop_class("PartsSingleModeSkillLearningListItem");
        let info_class = get_class("Gallop.PartsSingleModeSkillLearningListItem.Info");

        let get_top_info: GetObject = std::mem::transmute(get_method(
            parts_single_mode_skill_learning_list_item_class,
            "GetTopInfo",
            0,
        ));

        let skills = if plus {
            let info = get_top_info(item, null());
            let master_data = get_object(info_class, "MasterData", info);
            let mut skill_data = skilldata_to_struct(master_data);
            skill_data.level = get_i32(info_class, "Level", info);
            vec![skill_data]
        } else {
            let info_list = get_object_field(
                parts_single_mode_skill_learning_list_item_class,
                "_infoList",
                item,
            );
            let list_class = *(info_list as *mut *mut Il2CppClass);
            let size = get_i32_field(list_class, "_size", info_list);
            let info_array = get_pointer_field(list_class, "_items", info_list) as *mut Il2CppArray;
            let mut skill_vec: Vec<SkillData> = Vec::new();
            for i in 0..size as usize {
                let info = array_get_obj(info_array.as_ref_unchecked(), i);
                let master_data = get_object(info_class, "MasterData", info);
                let mut skill_data = skilldata_to_struct(master_data);
                skill_data.level = get_i32(info_class, "Level", info);
                skill_vec.push(skill_data);
            }
            skill_vec
        };

        return skills;
    }
}

unsafe extern "C" fn single_mode_skill_learning_view_controller_on_click_plus_list_item_hook(
    this: *mut Il2CppObject,
    item: *mut Il2CppObject,
    method_info: *mut MethodInfo,
) {
    unsafe {
        let vtable = VTABLE.unwrap();
        let tx = TX.clone();

        let (_, interceptor) = get_hachimi_and_interceptor();
        let trampoline = (vtable.interceptor_get_trampoline_addr)(
            interceptor,
            single_mode_skill_learning_view_controller_on_click_plus_list_item_hook as *mut c_void,
        );
        let original: OnClickListItem = std::mem::transmute(trampoline);

        let skill_data = on_click_list_item_common(item, true);

        let message = Message {
            message_type: MessageType::SkillPlus,
            message: MessageData::SkillUpdate(skill_data),
        };
        tx.send(serde_json::to_string(&message).unwrap()).unwrap();

        drop(message);
        drop(tx);

        return original(this, item, method_info);
    }
}

unsafe extern "C" fn single_mode_skill_learning_view_controller_on_click_minus_list_item_hook(
    this: *mut Il2CppObject,
    item: *mut Il2CppObject,
    method_info: *mut MethodInfo,
) {
    unsafe {
        let vtable = VTABLE.unwrap();
        let tx = TX.clone();

        let (_, interceptor) = get_hachimi_and_interceptor();
        let trampoline = (vtable.interceptor_get_trampoline_addr)(
            interceptor,
            single_mode_skill_learning_view_controller_on_click_minus_list_item_hook as *mut c_void,
        );
        let original: OnClickListItem = std::mem::transmute(trampoline);

        let skill_data = on_click_list_item_common(item, false);

        let message = Message {
            message_type: MessageType::SkillMinus,
            message: MessageData::SkillUpdate(skill_data),
        };
        tx.send(serde_json::to_string(&message).unwrap()).unwrap();

        drop(message);
        drop(tx);

        return original(this, item, method_info);
    }
}

fn evo_skill_common(original_id: i32, result_id: i32) {
    unsafe {
        let tx = TX.clone();

        let master_data_manager_class = get_gallop_class("MasterDataManager");
        let master_skill_data_class = get_gallop_class("MasterSkillData");

        let master_skill_data_get: unsafe extern "C" fn(
            *mut Il2CppObject,
            i32,
        ) -> *mut Il2CppObject = std::mem::transmute(get_method(master_skill_data_class, "Get", 1));

        let master_data_manager = get_singleton(master_data_manager_class);
        let master_skill_data = get_object(
            master_data_manager_class,
            "masterSkillData",
            master_data_manager,
        );

        let original_skill_data = master_skill_data_get(master_skill_data, original_id);
        let message = if result_id != 0 {
            let result_skill_data = master_skill_data_get(master_skill_data, result_id);
            let mut result_skill_struct = skilldata_to_struct(result_skill_data);
            result_skill_struct.group_id = original_id / 10;
            Message {
                message_type: MessageType::SkillPlus,
                message: MessageData::SkillUpdate(vec![result_skill_struct]),
            }
        } else {
            Message {
                message_type: MessageType::SkillPlus,
                message: MessageData::SkillUpdate(vec![skilldata_to_struct(original_skill_data)]),
            }
        };
        tx.send(serde_json::to_string(&message).unwrap()).unwrap();

        drop(message);
        drop(tx);
    }
}

unsafe extern "C" fn parts_single_mode_skill_upgrade_select_item_is_selected(
    this: *mut Il2CppObject,
) -> bool {
    unsafe {
        let vtable = VTABLE.unwrap();

        let (_, interceptor) = get_hachimi_and_interceptor();
        let trampoline = (vtable.interceptor_get_trampoline_addr)(
            interceptor,
            parts_single_mode_skill_upgrade_select_item_is_selected as *mut c_void,
        );
        let original: unsafe extern "C" fn(*mut Il2CppObject) -> bool =
            std::mem::transmute(trampoline);

        let parts_single_mode_skill_upgrade_select_item_class =
            get_gallop_class("PartsSingleModeSkillUpgradeSelectItem");
        let select_result_class =
            get_class("Gallop.DialogSingleModeSkillUpgradeSelect.SelectResult");

        let get_selected_result: unsafe extern "C" fn(*mut Il2CppObject) -> *mut Il2CppObject =
            std::mem::transmute(get_method(
                parts_single_mode_skill_upgrade_select_item_class,
                "GetSelectedResult",
                0,
            ));

        let is_selected = original(this);

        if is_selected {
            let selected_result = get_selected_result(this);
            let original_id = get_i32_field(select_result_class, "OriginSkillId", selected_result);
            let result_id = get_i32_field(select_result_class, "ResultSkillId", selected_result);
            evo_skill_common(original_id, result_id);
        }

        return is_selected;
    }
}

unsafe extern "C" fn dialog_single_mode_skill_upgrade_speciality_select_on_click_skill_upgrade_speciality_select_item_hook(
    this: *mut Il2CppObject,
    original_skill_id: i32,
    result_skill_id: i32,
) {
    unsafe {
        let vtable = VTABLE.unwrap();

        let (_, interceptor) = get_hachimi_and_interceptor();
        let trampoline = (vtable.interceptor_get_trampoline_addr)(
            interceptor,
            dialog_single_mode_skill_upgrade_speciality_select_on_click_skill_upgrade_speciality_select_item_hook as *mut c_void,
        );
        let original: unsafe extern "C" fn(*mut Il2CppObject, i32, i32) =
            std::mem::transmute(trampoline);

        evo_skill_common(original_skill_id, result_skill_id);

        original(this, original_skill_id, result_skill_id);
    }
}

fn websocket_handler(stream: TcpStream) {
    let mut ws = accept(stream).unwrap();
    let rx = RX.clone();
    let tx = TX.clone();
    let mut counter: u32 = 0;
    for msg in rx.iter() {
        counter += 1;
        ws.send(Binary(Bytes::copy_from_slice(&counter.to_le_bytes())))
            .unwrap();
        if let Ok(response) = ws.read() {
            if response.into_data().get_u32_le() != counter {
                unsafe {
                    log(0, format!("Socket closed").as_str());
                }
                tx.send(msg).unwrap();
                return;
            }
        } else {
            unsafe {
                log(0, format!("Socket closed").as_str());
            }
            tx.send(msg).unwrap();
            return;
        }

        unsafe {
            log(0, format!("Sending message: {counter}").as_str());
        }
        ws.send(Text(Utf8Bytes::from(msg))).unwrap();
    }
}

fn web_server_thread() {}

#[unsafe(export_name = "hachimi_init")]
pub unsafe extern "C" fn hachimi_init(vtable: *const Vtable, version: i32) -> InitResult {
    if vtable.is_null() {
        return InitResult::Error;
    }
    if version < VERSION {
        return InitResult::Error;
    }

    unsafe {
        VTABLE = Some(&*vtable);
        let vtable = VTABLE.unwrap();
        il2cpp::helper::init(vtable);

        log(0, "Hooking started");
        let (_, interceptor) = get_hachimi_and_interceptor();

        let orig_addr = get_method(
            get_gallop_class("DialogTrainedCharacterDetail"),
            "CreateSetupParameter",
            3,
        );
        (vtable.interceptor_hook)(
            interceptor,
            orig_addr,
            dialog_trained_character_detail_create_setup_parameter_hook as *mut c_void,
        );

        let orig_addr = get_method(
            get_gallop_class("PartsSingleModeCharacterStatusPanel"),
            "Setup",
            1,
        );
        (vtable.interceptor_hook)(
            interceptor,
            orig_addr,
            parts_single_mode_character_status_panel_setup_hook as *mut c_void,
        );

        let orig_addr = get_method(
            get_gallop_class("SingleModeMainViewTrainingStatus"),
            "Setup",
            1,
        );
        (vtable.interceptor_hook)(
            interceptor,
            orig_addr,
            single_mode_main_view_training_status_setup_hook as *mut c_void,
        );

        let orig_addr = get_method(
            get_gallop_class("SingleModeSkillLearningViewController"),
            "OnClickPlusListItem",
            1,
        );
        (vtable.interceptor_hook)(
            interceptor,
            orig_addr,
            single_mode_skill_learning_view_controller_on_click_plus_list_item_hook as *mut c_void,
        );

        let orig_addr = get_method(
            get_gallop_class("SingleModeSkillLearningViewController"),
            "OnClickMinusListItem",
            1,
        );
        (vtable.interceptor_hook)(
            interceptor,
            orig_addr,
            single_mode_skill_learning_view_controller_on_click_minus_list_item_hook as *mut c_void,
        );

        let orig_addr = get_method(
            get_gallop_class("PartsSingleModeSkillUpgradeSelectItem"),
            "IsSelected",
            0,
        );
        (vtable.interceptor_hook)(
            interceptor,
            orig_addr,
            parts_single_mode_skill_upgrade_select_item_is_selected as *mut c_void,
        );

        // Global doesn't have evolved skills, so we don't need this just yet :)
        /*
        let orig_addr = get_method(
            get_gallop_class("DialogSingleModeSkillUpgradeSpecialitySelect"),
            "OnClickSkillUpgradeSpecialitySelectItem",
            2,
        );
        (vtable.interceptor_hook)(
            interceptor,
            orig_addr,
            dialog_single_mode_skill_upgrade_speciality_select_on_click_skill_upgrade_speciality_select_item_hook as *mut c_void,
        );
        */

        log(0, "Hooking finished");

        let sprite_class = get_class_from_image("UnityEngine.CoreModule.dll", "UnityEngine.Sprite");
        let sprite_get_name: unsafe extern "C" fn(*mut Il2CppObject) -> *mut Il2CppString =
            std::mem::transmute(get_method(sprite_class, "get_name", 0));
        let ui_manager = UiManager::init();
        log(0, "Loading atlas");
        let atlas_reference = AtlasReference::new(ui_manager.load_atlas(15, true));
        log(0, "Getting sprites");
        let rank_sprite_array = atlas_reference.get_sprites();
        log(0, "Iterating over sprites");
        let mut rank_icons = HashMap::new();
        for i in 0..(*rank_sprite_array).max_length {
            let sprite = array_get_obj(rank_sprite_array.as_ref_unchecked(), i);
            let sprite_name = sprite_get_name(sprite);
            let sprite_name_string = il2cppstring_as_string(sprite_name.as_ref_unchecked());
            let sprite_name_split: Vec<&str> = sprite_name_string.split("_").collect();
            let sprite_index: i32 = sprite_name_split.last().unwrap().parse().unwrap();
            let texture2d = sprite_to_texture2d(sprite);
            let png = texture2d_to_png(texture2d);
            rank_icons.insert(sprite_index + 1, png);
        }

        // let resource_manager_class = get_class("Gallop.ResourceManager");
        // let load_on_view = get_method(
        //     resource_manager_class,
        //     "LoadOnView<UnityEngine::Texture>",
        //     2,
        // );

        log(0, "Spawning websocket server thread");
        let ws_server = TcpListener::bind("127.0.0.1:0").unwrap();
        let ws_port = ws_server.local_addr().unwrap().port();
        thread::spawn(move || {
            log(0, "Starting websocket server thread");

            for stream in ws_server.incoming() {
                log(0, "Got connection");
                thread::spawn(|| websocket_handler(stream.unwrap()));
            }
        });

        log(0, "Spawning web server thread");
        thread::spawn(move || {
            log(0, "Starting web server thread");

            rouille::start_server("127.0.0.1:5555", move |request| {
                rouille::router!(request,
                    (GET) (/socket) => {
                        rouille::Response::text(ws_port.to_string())
                    },
                    (GET) (/rank/{rank_score: i32}) => {
                        rouille::Response::text(serde_json::to_string(&get_total_rank(rank_score)).unwrap())
                    },
                    (GET) (/rank_icon/{rank_score: i32}) => {
                        if let Some(icon) = rank_icons.get(&get_total_rank(rank_score)) {
                            rouille::Response::from_data("image/png", icon.clone())
                        } else {
                            rouille::Response::empty_400()
                        }
                        // let sprite = get_final_training_rank_sprite(get_total_rank(rank_score));
                        // let texture2d = sprite_to_texture2d(sprite);
                        // let png = texture2d_to_png(texture2d);
                        // rouille::Response::from_data("image/png", png.clone())
                    },
                    _ => {
                        rouille::match_assets(&request, "uisc")
                    }
                )
            });
        });
    }

    InitResult::Ok
}
