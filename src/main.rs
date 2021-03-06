
extern crate libc;
extern crate openal;
extern crate cocoa;
extern crate time;
extern crate hyper;
extern crate block;
extern crate rustc_serialize;
#[macro_use]
extern crate objc;

use std::option::Option;
use std::thread;
use std::io::Read;
use std::sync::{Once, ONCE_INIT};
use std::string::String;
use std::fs::File;

use libc::{c_void};
use core_foundation::*;
use objc::*;
use objc::runtime::*;
use cocoa::base::{class,id,nil};
use cocoa::foundation::{NSAutoreleasePool, NSString};
use cocoa::appkit::{NSApp,NSApplication};

use hyper::Client;
use hyper::header::{Connection};
use hyper::status::StatusCode;

use self::block::{ConcreteBlock};
use rustc_serialize::json;

mod core_graphics;
mod core_foundation;
mod alut;
mod event_tap;
mod tickeys;
mod cocoa_ext;

use tickeys::{Tickeys, AudioScheme};
use cocoa_ext::{NSUserNotification, RetainRelease};


const CURRENT_VERSION : &'static str = "0.3.5";
const OPEN_SETTINGS_KEY_SEQ: &'static[u8] = &[12, 0, 6, 18, 19, 20]; //QAZ123
//todo: what's the better way to store constants?
const WEBSITE : &'static str = "http://www.yingdev.com/projects/tickeys";
const DONATE_URL: &'static str = "http://www.yingdev.com/home/donate";
const CHECK_UPDATE_API : &'static str = "http://www.yingdev.com/projects/latestVersion?product=Tickeys";

static mut SHOWING_GUI:bool = false;

fn main() 
{	
	unsafe{NSAutoreleasePool::new(nil)};

	request_accessiblility();	
	begin_check_for_update(CHECK_UPDATE_API);
	
	let pref = Pref::load();

	let mut tickeys = Tickeys::new();
	tickeys.load_scheme(&get_data_path(&pref.audio_scheme), &find_scheme(&pref.audio_scheme, &load_audio_schemes()));
	tickeys.set_volume(pref.volume);
	tickeys.set_pitch(pref.pitch);
	tickeys.set_on_keydown(Some(handle_keydown)); //handle qaz123
	tickeys.start();

	show_notification("Tickeys正在运行", "按 QAZ123 打开设置");

	app_run();
}

fn request_accessiblility()
{		
	println!("request_accessiblility");

	#[link(name = "ApplicationServices", kind = "framework")]
	extern "system"
	{
	 	fn AXIsProcessTrustedWithOptions (options: id) -> bool;
	}

 	unsafe fn is_enabled(prompt: bool) -> bool
 	{ 
		let dict:id = msg_send![class("NSDictionary"), dictionaryWithObject:(if prompt {kCFBooleanTrue}else{kCFBooleanFalse}) forKey:kAXTrustedCheckOptionPrompt];
		dict.autorelease();
		return AXIsProcessTrustedWithOptions(dict);
	}

	unsafe
	{
		if is_enabled(false) {return;}

		while !is_enabled(true)
		{
			let alert:id = msg_send![class("NSAlert"), new];
			alert.autorelease();
			let _:id = msg_send![alert, setMessageText: NSString::alloc(nil).init_str("您必须将Tickeys.app添加到 系统偏好设置 > 安全与隐私 > 辅助功能 列表中并√，否则Tickeys无法工作")];
			let _:id = msg_send![alert, addButtonWithTitle: NSString::alloc(nil).init_str("退出")];
			let _:id = msg_send![alert, addButtonWithTitle: NSString::alloc(nil).init_str("我已照做，继续运行！")];
			
			let btn:i32 = msg_send![alert, runModal];
			println!("request_accessiblility alert: {}", btn);
			match btn
			{
				1001 => {continue},
				1002 => {app_terminate();},
				_ => {panic!("request_accessiblility");}
			}
		}

		app_relaunch_self();
	}
}

fn load_audio_schemes() -> Vec<AudioScheme>
{
	let path = get_res_path("data/schemes.json");
	let mut file = File::open(path).unwrap();

	let mut json_str = String::with_capacity(512);
	match file.read_to_string(&mut json_str)
	{
		Ok(_) => {},
		Err(e) => panic!("Failed to read json:{}",e)
	}

	let schemes:Vec<AudioScheme> = json::decode(&json_str).unwrap();

	schemes
}

fn get_res_path(sub_path: &str) -> String
{
	let args:Vec<_> = std::env::args().collect();
	let mut data_path = std::path::PathBuf::from(&args[0]);
	data_path.pop();
	data_path.push("../Resources/");
	data_path.push(sub_path);

	data_path.into_os_string().into_string().unwrap()
}

fn get_data_path(sub_path: &str) -> String
{
	get_res_path(&("data/".to_string() + sub_path))
}

fn find_scheme<'a>(name: &str, from: &'a Vec<AudioScheme>) -> &'a AudioScheme
{
	from.iter().filter(|s|{ *(s.name) == *name}).next().unwrap()
}

fn begin_check_for_update(url: &str)
{
	#[derive(RustcDecodable, RustcEncodable)]
	#[allow(non_snake_case)]
	struct Version
	{
		Version: String
	}

	let run_loop_ref = unsafe{CFRunLoopGetCurrent() as usize};

	let check_update_url = url.to_string();

	thread::spawn(move ||
	{
	    let mut client = Client::new();

	    let result = client.get(&check_update_url)
	        .header(Connection::close())
	        .send();
	    
	    let mut resp;
	    match result
	    {
	    	Ok(r) => resp = r,
	    	Err(e) => {
	    		println!("Failed to check for update: {}", e);
	    		return;
	    	}
	    }

	    if resp.status == StatusCode::Ok
	    {
	    	let mut content = String::new();
	    	match resp.read_to_string(&mut content)
	    	{
	    		Ok(_) => {},
	    		Err(e) => {
	    			println!("Failed to read http content: {}", e);
	    			return;
	    		}
	    	}
	    	println!("Response: {}", content);
	    	
	    	if content.contains("Version")
	    	{		    	
	    		let ver:Version = json::decode(&content).unwrap();
	    		println!("ver={}",ver.Version);
	    		if ver.Version != CURRENT_VERSION
	    		{
	    			let cblock : ConcreteBlock<(),(),_> = ConcreteBlock::new(move ||
			    	{
			    		println!("New Version Available!");
			    		let info_str = format!("{} -> {}", CURRENT_VERSION, ver.Version);
			    		show_notification("新版本可用!", &info_str);
			    	});
			    	
			    	let block = & *cblock.copy();

			    	unsafe
			    	{
			    		CFRunLoopPerformBlock(run_loop_ref as *mut c_void, kCFRunLoopDefaultMode, block);
			    	}
		    	}
	    	}

	    }else
	    {
	    	println!("Failed to check for update: Status {}", resp.status);
	    }
	});
}

fn handle_keydown(tickeys: &Tickeys, _:u8)
{
	let last_keys = tickeys.get_last_keys();
	let last_keys_len = last_keys.len();
	let seq_len = OPEN_SETTINGS_KEY_SEQ.len();

	if last_keys_len < seq_len {return;}

	//cmp from tail to head
	for i in 1..(seq_len+1)
	{
		if last_keys[last_keys_len - i] != OPEN_SETTINGS_KEY_SEQ[seq_len - i]
		{
			return;
		}
	}

	show_settings(tickeys);
}

fn show_settings(tickeys: &Tickeys)
{
	println!("Settings!");

	unsafe
	{
		if SHOWING_GUI
		{
			return;
		}
		SHOWING_GUI = true;
		SettingsDelegate::new(nil, std::mem::transmute(tickeys));
	}
}

fn show_notification(title: &str, msg: &str)
{
	static REGISTER_DELEGATE: Once = ONCE_INIT;
	REGISTER_DELEGATE.call_once(||
	{
		unsafe
		{
			let noti_center_del:id = UserNotificationCenterDelegate::new(nil).autorelease();
			let center:id = msg_send![class("NSUserNotificationCenter"), defaultUserNotificationCenter];
			let _:id = msg_send![center, setDelegate: noti_center_del];
		}
	});

	unsafe
	{
		let note:id = NSUserNotification::new(nil).autorelease();
		note.setTitle(NSString::alloc(nil).init_str(title));
		note.setInformativeText(NSString::alloc(nil).init_str(msg));
		
		let center:id = msg_send![class("NSUserNotificationCenter"), defaultUserNotificationCenter];

		msg_send![center, deliverNotification: note]
	}
}

fn app_run()
{
	unsafe
	{
		let app = NSApp();
		app.run();
	}
}

fn app_relaunch_self()
{
	unsafe
	{
		let bundle:id = msg_send![class("NSBundle"),mainBundle];
		let path:id = msg_send![bundle,  executablePath];

		let proc_info:id = msg_send![class("NSProcessInfo"), processInfo];
		let proc_id:i32 = msg_send![proc_info, processIdentifier];
		let proc_id_str:id = NSString::alloc(nil).init_str(&format!("{}",proc_id)).autorelease();

		let args:id = msg_send![class("NSMutableArray"), new];

		let _:id = msg_send![args, addObject:path];

		let _:id = msg_send![args, addObject:proc_id_str];

		let _:id = msg_send![class("NSTask"), launchedTaskWithLaunchPath:path arguments:args];

	}

	std::process::exit(0);
}

fn app_terminate()
{
	unsafe
	{
		msg_send![NSApp(), terminate:nil]
	}
}


struct Pref
{
	audio_scheme: String,
	volume: f32,
	pitch: f32,
}

impl Pref
{
	fn load() -> Pref
	{
		unsafe
		{		
			let user_defaults: id = msg_send![class("NSUserDefaults"), standardUserDefaults];
			let pref_exists_key:id = NSString::alloc(nil).init_str("pref_exists");
					
			//todo: 每次都要加载？
			let schemes = load_audio_schemes();

			let pref = Pref{audio_scheme: schemes[0].name.clone(), volume: 0.5f32, pitch: 1.0f32};

			let pref_exists: id = msg_send![user_defaults, stringForKey: pref_exists_key];
			if pref_exists == nil //first run 
			{
				pref.save();
				return pref;
			}else
			{
				let audio_scheme: id = msg_send![user_defaults, stringForKey:NSString::alloc(nil).init_str("audio_scheme")];
				let volume: f32 = msg_send![user_defaults, floatForKey: NSString::alloc(nil).init_str("volume")];
				let pitch: f32 = msg_send![user_defaults, floatForKey: NSString::alloc(nil).init_str("pitch")];
				
				let len:usize = msg_send![audio_scheme, length];
				
				let mut scheme_bytes:Vec<u8> = Vec::with_capacity(len);
        		scheme_bytes.set_len(len);
       			std::ptr::copy_nonoverlapping(audio_scheme.UTF8String() as *const u8, scheme_bytes.as_mut_ptr(), len);
				let mut scheme_str = String::from_utf8(scheme_bytes).unwrap();

				//validate scheme
				if schemes.iter().filter(|s|{*s.name == scheme_str}).count() == 0
				{
					scheme_str = pref.audio_scheme;
				}
				
				Pref{audio_scheme:  scheme_str, volume: volume, pitch: pitch}
			}
		}
		
	}

	fn save(&self)
	{
		unsafe
		{
			let user_defaults: id = msg_send![class("NSUserDefaults"), standardUserDefaults];

			let _:id = msg_send![user_defaults, setObject: NSString::alloc(nil).init_str(&self.audio_scheme) forKey: NSString::alloc(nil).init_str("audio_scheme")];
			let _:id = msg_send![user_defaults, setFloat: self.volume forKey: NSString::alloc(nil).init_str("volume")];
			let _:id = msg_send![user_defaults, setFloat: self.pitch forKey: NSString::alloc(nil).init_str("pitch")];

			let pref_exists_key:id = NSString::alloc(nil).init_str("pref_exists");
			let _:id = msg_send![user_defaults, setObject:pref_exists_key forKey: pref_exists_key];

			let _:id = msg_send![user_defaults, synchronize];
		}


	}
}


#[allow(non_snake_case)]
#[allow(unused_variables)]
pub trait UserNotificationCenterDelegate //: <NSUserNotificationCenerDelegate>
{
	fn new(_: Self) -> id
	{
		static REGISTER_APPDELEGATE: Once = ONCE_INIT;
		REGISTER_APPDELEGATE.call_once(||
		{
			let nsobjcet = objc::runtime::Class::get("NSObject").unwrap();
			let mut decl = objc::declare::ClassDecl::new(nsobjcet, "UserNotificationCenterDelegate").unwrap();

			unsafe
			{
				let delivered_fn: extern fn(&mut Object, Sel, id, id) = Self::userNotificationCenterDidDeliverNotification;
				decl.add_method(sel!(userNotificationCenter:didDeliverNotification:), delivered_fn);

				let activated_fn: extern fn(&mut Object, Sel, id, id) = Self::userNotificationCenterDidActivateNotification;
				decl.add_method(sel!(userNotificationCenter:didActivateNotification:), activated_fn);
			}

			decl.register();
		});

	    let cls = Class::get("UserNotificationCenterDelegate").unwrap();
	    unsafe 
	    {
	        msg_send![cls, new]
    	}
	}

	extern fn userNotificationCenterDidDeliverNotification(this: &mut Object, _cmd: Sel, center: id, note: id)
	{
		println!("userNotificationCenterDidDeliverNotification");
	}
	
	extern fn userNotificationCenterDidActivateNotification(this: &mut Object, _cmd: Sel, center: id, note: id)
	{
		println!("userNotificationCenterDidActivateNotification");

		unsafe
		{
			let workspace: id = msg_send![class("NSWorkspace"), sharedWorkspace];
			//todo: extract
			let url:id = msg_send![class("NSURL"), URLWithString: NSString::alloc(nil).init_str(WEBSITE)];

			let ok:bool = msg_send![workspace, openURL: url];

			msg_send![center, removeDeliveredNotification:note]
		}
	}
}

impl UserNotificationCenterDelegate for id
{

}


#[allow(non_snake_case)]
#[allow(unused_variables)]
trait SettingsDelegate
{
	fn new(_:Self, ptr_to_app: *mut Tickeys) -> id
	{
		static REGISTER_APPDELEGATE: Once = ONCE_INIT;
		REGISTER_APPDELEGATE.call_once(||
		{
			println!("SettingsDelegate::new::REGISTER_APPDELEGATE");
			let nsobjcet = objc::runtime::Class::get("NSObject").unwrap();
			let mut decl = objc::declare::ClassDecl::new(nsobjcet, "SettingsDelegate").unwrap();

			unsafe
			{
				//property ptr_to_app
				decl.add_ivar::<usize>("_user_data");
				let set_user_data_fn: extern fn(&mut Object, Sel, usize) = Self::set_user_data_;
				decl.add_method(sel!(setUser_data:), set_user_data_fn);

				let get_user_data_fn: extern fn(&Object, Sel)->usize = Self::get_user_data_;
				decl.add_method(sel!(user_data), get_user_data_fn);

				//property popup_audio_scheme
				decl.add_ivar::<id>("_popup_audio_scheme");
				let set_popup_audio_scheme_fn: extern fn(&mut Object, Sel, id) = Self::set_popup_audio_scheme_;
				decl.add_method(sel!(setPopup_audio_scheme:), set_popup_audio_scheme_fn);

				let get_popup_audio_scheme_fn: extern fn(&Object, Sel)->id = Self::get_popup_audio_scheme_;
				decl.add_method(sel!(popup_audio_scheme), get_popup_audio_scheme_fn);

				//property slide_volume
				decl.add_ivar::<id>("_slide_volume");
				let set_slide_volume_fn: extern fn(&mut Object, Sel, id) = Self::set_slide_volume_;
				decl.add_method(sel!(setSlide_volume:), set_slide_volume_fn);

				let get_slide_volume_fn: extern fn(&Object, Sel)->id = Self::get_slide_volume_;
				decl.add_method(sel!(slide_volume), get_slide_volume_fn);

				//property slide_pitch
				decl.add_ivar::<id>("_slide_pitch");
				let set_slide_pitch_fn: extern fn(&mut Object, Sel, id) = Self::set_slide_pitch_;
				decl.add_method(sel!(setSlide_pitch:), set_slide_pitch_fn);

				let get_slide_pitch_fn: extern fn(&Object, Sel)->id = Self::get_slide_pitch_;
				decl.add_method(sel!(slide_pitch), get_slide_pitch_fn);

				//property label_version
				decl.add_ivar::<id>("_label_version");
				let set_label_version_fn: extern fn(&mut Object, Sel, id) = Self::set_label_version_;
				decl.add_method(sel!(setLabel_version:), set_label_version_fn);				

				let get_label_version_fn: extern fn(&Object, Sel)->id = Self::get_label_version_;
				decl.add_method(sel!(label_version), get_label_version_fn);

				//property window
				decl.add_ivar::<id>("_window");
				let set_window_fn: extern fn(&mut Object, Sel, id) = Self::set_window_;
				decl.add_method(sel!(setWindow:), set_window_fn);

				let get_window_fn: extern fn(& Object, Sel)->id = Self::get_window_;
				decl.add_method(sel!(getWindow), get_window_fn);

				//methods
				let quit_fn: extern fn(&mut Object, Sel, id) = Self::quit_;
				decl.add_method(sel!(quit:), quit_fn);

				let value_changed_fn: extern fn(&mut Object, Sel, id) = Self::value_changed_;
				decl.add_method(sel!(value_changed:), value_changed_fn);

				let follow_link_fn: extern fn(&mut Object, Sel, id) = Self::follow_link_;
				decl.add_method(sel!(follow_link:), follow_link_fn);

				let windowWillClose_fn: extern fn(&Object, Sel, id) = Self::windowWillClose;
				decl.add_method(sel!(windowWillClose:), windowWillClose_fn);

				//let windowDidBecomeKey_fn: extern fn(&mut Object,Sel,id) = Self::windowDidBecomeKey;
				//decl.add_method(sel!(windowDidBecomeKey:), windowDidBecomeKey_fn);
			}

			decl.register();
		});


	    let cls = Class::get("SettingsDelegate").unwrap();
	    unsafe 
	    {
	       	let obj: id = msg_send![cls, new];	       
	       	obj.retain();
	       	let _:id = msg_send![obj, setUser_data: ptr_to_app];

	       	let data: *mut Tickeys = msg_send![obj, user_data];
	       	assert!(data == ptr_to_app);

			let nib_name = NSString::alloc(nil).init_str("Settings");
			let _: id = msg_send![class("NSBundle"), loadNibNamed:nib_name owner: obj];	

			Self::load_values(obj);

	       obj
    	}    
	}

	//property ptr_to_app
	extern fn set_user_data_(this: &mut Object, _cmd: Sel, val: usize){unsafe { this.set_ivar::<usize>("_user_data", val); }}
	extern fn get_user_data_(this: &Object, _cmd: Sel) -> usize{unsafe { *this.get_ivar::<usize>("_user_data") }}

	//property popup_audio_scheme
	extern fn set_popup_audio_scheme_(this: &mut Object, _cmd: Sel, val: id){unsafe { this.set_ivar::<id>("_popup_audio_scheme", val); }}
	extern fn get_popup_audio_scheme_(this: &Object, _cmd: Sel) -> id{unsafe { *this.get_ivar::<id>("_popup_audio_scheme") }}

	//property slide_volume
	extern fn set_slide_volume_(this: &mut Object, _cmd:Sel, val: id){unsafe{this.set_ivar::<id>("_slide_volume", val);}}
	extern fn get_slide_volume_(this: &Object, _cmd:Sel) -> id{unsafe{*this.get_ivar::<id>("_slide_volume")}}

	//property slide_pitch
	extern fn set_slide_pitch_(this: &mut Object, _cmd:Sel, val: id){unsafe{this.set_ivar::<id>("_slide_pitch", val);}}
	extern fn get_slide_pitch_(this: &Object, _cmd:Sel) -> id{unsafe{*this.get_ivar::<id>("_slide_pitch")}}

	//property label_version
	extern fn set_label_version_(this: &mut Object, _cmd: Sel, val: id){unsafe{this.set_ivar::<id>("_label_version", val);}}
	extern fn get_label_version_(this: &Object, _cmd: Sel)->id{unsafe{*this.get_ivar::<id>("_label_version")}}
	
	//property window
	extern fn set_window_(this: &mut Object, _cmd: Sel, val: id){unsafe{this.set_ivar::<id>("_window", val);}}
	extern fn get_window_(this: &Object, _cmd: Sel)->id{unsafe{*this.get_ivar::<id>("_window")}}

	extern fn quit_(this: &mut Object, _cmd: Sel, sender: id)
	{
		println!("Quit");
		app_terminate();
	}

	extern fn follow_link_(this: &mut Object, _cmd: Sel, sender: id)
	{
		unsafe
		{
			let tag:i64 = msg_send![sender, tag];
			let url = match tag
			{
				0 => WEBSITE,
				1 => DONATE_URL,
				_ => panic!("SettingsDelegate::follow_link_")
			};

			let workspace: id = msg_send![class("NSWorkspace"), sharedWorkspace];
			let url:id = msg_send![class("NSURL"), 
			URLWithString: NSString::alloc(nil).init_str(url)];

			msg_send![workspace, openURL: url]
		}
	}

	extern fn value_changed_(this: &mut Object, _cmd:Sel, sender: id)
	{
		println!("SettingsDelegate::value_changed_");

		const TAG_POPUP_SCHEME: i64 = 0;
		const TAG_SLIDE_VOLUME: i64 = 1; 
		const TAG_SLIDE_PITCH: i64 = 2;

		unsafe
		{
			let user_defaults: id = msg_send![class("NSUserDefaults"), standardUserDefaults];
			let tickeys_ptr:&mut Tickeys = msg_send![this, user_data];
			let tag:i64 = msg_send![sender, tag];
			
			match tag
			{
				TAG_POPUP_SCHEME => 
				{

					let value:i32 = msg_send![sender, indexOfSelectedItem];
					
					let schemes = load_audio_schemes();
					let sch = &schemes[value as usize];

					let mut scheme_dir = "data/".to_string() + &sch.name;//.to_string();
					//scheme_dir.push_str(&sch.name);
					tickeys_ptr.load_scheme(&get_res_path(&scheme_dir), sch);

					let _:id = msg_send![user_defaults, setObject: NSString::alloc(nil).init_str(sch.name.as_ref()) 
														   forKey: NSString::alloc(nil).init_str("audio_scheme")];
				},

				TAG_SLIDE_VOLUME =>
				{
					let value:f32 = msg_send![sender, floatValue];
					tickeys_ptr.set_volume(value);

					let _:id = msg_send![user_defaults, setFloat: value forKey: NSString::alloc(nil).init_str("volume")];
				},

				TAG_SLIDE_PITCH =>
				{
					let mut value:f32 = msg_send![sender, floatValue];
					if value > 1f32
					{
						//just map [1, 1.5] -> [1, 2]
						value = value * (2.0f32/1.5f32);
					}
					tickeys_ptr.set_pitch(value);

					let _:id = msg_send![user_defaults, setFloat: value forKey: NSString::alloc(nil).init_str("pitch")];
				}

				_ => {panic!("WTF");}
			}
		}
		
	}

	extern fn windowWillClose(this: &Object, _cmd: Sel, note: id)
	{
		println!("SettingsDelegate::windowWillClose");
		unsafe
		{
			let app_ptr: *mut Tickeys = msg_send![this, user_data];
			SHOWING_GUI = false;

			let user_defaults: id = msg_send![class("NSUserDefaults"), standardUserDefaults];
			let _:id = msg_send![user_defaults, synchronize];
			let _:id = msg_send![this, release];
		}
	}

	unsafe fn load_values(this: id)
	{
		println!("loadValues");
		let user_defaults: id = msg_send![class("NSUserDefaults"), standardUserDefaults];
		let popup_audio_scheme: id = msg_send![this, popup_audio_scheme];
		let _: id = msg_send![popup_audio_scheme, removeAllItems];
		
		let pref = Pref::load();
		let schemes = load_audio_schemes();
		

		for i in 0..schemes.len()
		{
			let s = &schemes[i];

			let _: id = msg_send![popup_audio_scheme, addItemWithTitle: NSString::alloc(nil).init_str(&s.display_name)];
			if  *s.name == pref.audio_scheme
			{
				let _:id = msg_send![popup_audio_scheme, selectItemAtIndex:i];
			}
		}

		let slide_volume: id = msg_send![this, slide_volume];
		let _:id = msg_send![slide_volume, setFloatValue: pref.volume];

		let slide_pitch: id = msg_send![this, slide_pitch];
		let value =  if pref.pitch > 1f32
		{
			pref.pitch * (1.5f32/2.0f32)	
		} else
		{
			pref.pitch
		};
		let _:id = msg_send![slide_pitch, setFloatValue: value];

		let label_version: id = msg_send![this, label_version];
		let _:id = msg_send![label_version, setStringValue:NSString::alloc(nil).init_str(format!("v{}",CURRENT_VERSION).as_ref())];

		//let _:id = msg_send![this, show]

		println!("makeKeyAndOrderFront:");
		let win:id = msg_send![this, getWindow];		
		let _:id = msg_send![win, makeKeyAndOrderFront:nil];
		let _:id = msg_send![NSApp(), activateIgnoringOtherApps:true];
	}

}

impl SettingsDelegate for id
{
}





