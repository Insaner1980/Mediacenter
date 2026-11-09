//! Main-window-only remote setup. Secrets are returned only by explicit pairing.
use super::{network::NetworkInterface, protocol::{RemoteError,ErrorCode}};
use serde::{Serialize,Deserialize};
use ts_rs::TS;
use std::{path::{Path,PathBuf},sync::Arc,net::SocketAddr};
use super::{adapters::checked,discovery::Advertisement,network,server::{BindAddresses,RemoteServer,ServerRuntime,default_port},store::RemoteStore,tls::{self,IdentityNamespace,ServerIdentity},protocol::Id};
use crate::{app_state::AppState,control::ControlServices};
#[derive(Clone,Serialize,TS)]
#[serde(rename_all="camelCase")]
pub struct RemoteSettingsSnapshot {
 #[ts(type="number")] pub revision:u64,
 pub enabled:bool,pub listening:bool,pub advertising:bool,pub interface_name:Option<String>,pub port:u16,
 pub interfaces:Vec<NetworkInterface>,pub addresses:Vec<String>,pub requires_repairing:bool,pub status:String,pub pairing_attempt_id:Option<String>,
}
#[derive(Deserialize,TS)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct RemoteEnableRequest {#[ts(type="number")] pub revision:u64,pub enabled:bool,pub interface_name:Option<String>,pub port:u16}
#[derive(Serialize,TS)]
#[serde(rename_all="camelCase")]
pub struct RemotePairingDisplay {pub attempt_id:String,pub qr_rows:Vec<String>,pub addresses:Vec<String>,pub port:u16,pub manual_code:String,pub verification_words:Vec<String>,#[ts(type="number")]pub expires_in_ms:u64}
#[derive(Serialize,TS)]
#[serde(rename_all="camelCase")]
pub struct RemoteDevice {pub device_id:String,pub label:String,#[ts(type="number")]pub created_at_ms:u64,#[ts(type="number | null")]pub last_seen_at_ms:Option<u64>}
#[derive(Clone,Serialize,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
struct NetworkConfig {version:u8,revision:u64,interface_name:String,port:u16}
impl NetworkConfig {
 fn validate(&self)->Result<(),RemoteError>{
  if self.version!=1||self.revision==0||self.revision>9_007_199_254_740_991||self.port==0||self.interface_name.is_empty()||self.interface_name.len()>64||!self.interface_name.bytes().all(|b|b.is_ascii_alphanumeric()||b"_.:-".contains(&b)){return Err(config_error());}Ok(())
 }
 fn read(path:&Path)->Result<Option<Self>,RemoteError>{
  let bytes=tls::read_private(path).map_err(|_|config_error())?;
  bytes.map(|bytes|{let value:Self=serde_json::from_slice(&bytes).map_err(|_|config_error())?;value.validate()?;Ok(value)}).transpose()
 }
 fn save(&self,path:&Path)->Result<(),RemoteError>{
  self.validate()?;
  tls::write_private(path,&serde_json::to_vec(self).map_err(|_|config_error())?).map_err(|_|config_error())
 }
}
fn config_error()->RemoteError{RemoteError::new(ErrorCode::Unavailable,"Remote network settings are missing or invalid. Disable remote control, choose a trusted network and port, then enable it again.")}
fn unavailable()->RemoteError{RemoteError::new(ErrorCode::Unavailable,"Remote control is unavailable. Restart Mediacenter or review its private remote identity files.")}
struct SetupState {
 runtime:Option<Arc<ServerRuntime>>,config:Option<NetworkConfig>,enabled:bool,revision:u64,status:String,
 requires_repairing:bool,advertisement:Option<Advertisement>,suspended:bool,network_epoch:Option<u64>,
}
#[derive(Clone)]
pub struct RemoteSetup {inner:Arc<tokio::sync::Mutex<SetupState>>,server:Arc<RemoteServer>,state:AppState,path:PathBuf,opposite:PathBuf,default_port:u16,other_default_port:u16,name:&'static str}
impl RemoteSetup {
 /// Construction is local-only and fallible remote state never aborts local startup.
 pub fn new(app:Option<tauri::AppHandle>,state:AppState,server:Arc<RemoteServer>,namespace:IdentityNamespace)->Self {
  let other=match namespace{IdentityNamespace::Production=>IdentityNamespace::Development,IdentityNamespace::Development=>IdentityNamespace::Production};
  let path=state.data_dir.join(namespace.directory()).join("network-v1.json");
  let opposite=state.data_dir.join(other.directory()).join("network-v1.json");
  let initialized=(||{
   let identity=Arc::new(ServerIdentity::load_or_create(&state.data_dir,namespace,&mut state.db.lock().map_err(|_|unavailable())?,crate::app_state::now_ms() as i64)?);
   let requires_repairing=identity.requires_repairing();
   let preferences=RemoteStore::new(&mut state.db.lock().map_err(|_|unavailable())?,identity.id()).preferences().map_err(|_|unavailable())?;
   let services=match app{Some(app)=>ControlServices::new(app,state.clone(),identity.id().clone()),None=>ControlServices::native(state.clone(),identity.id().clone())}.map_err(|_|unavailable())?;
   let runtime=ServerRuntime::new(identity,services)?;
   let (config,status)=match NetworkConfig::read(&path){Ok(config)=>(config,if preferences.enabled{"Checking the selected local network.".into()}else{"Remote control is off.".into()}),Err(error)=>(None,error.to_string())};
   Ok::<_,RemoteError>(SetupState{runtime:Some(runtime),config,enabled:preferences.enabled,revision:preferences.revision.value(),status,requires_repairing,advertisement:None,suspended:false,network_epoch:None})
  })();
  let inner=initialized.unwrap_or_else(|_|SetupState{runtime:None,config:None,enabled:false,revision:1,status:"Remote control could not initialize securely. Local media remains available. Restart Mediacenter or review its private remote identity files.".into(),requires_repairing:false,advertisement:None,suspended:true,network_epoch:None});
  Self{inner:Arc::new(tokio::sync::Mutex::new(inner)),server,state,path,opposite,default_port:default_port(namespace),other_default_port:default_port(other),name:match namespace{IdentityNamespace::Production=>"Mediacenter",IdentityNamespace::Development=>"Mediacenter (development)"}}
 }
 async fn stop_locked(&self,inner:&mut SetupState){
  // Listener cancellation closes sockets and the active pairing attempt first.
  self.server.stop().await;
  if let Some(advertisement)=inner.advertisement.take(){advertisement.stop().await;}
 }
 fn save_enabled(&self,inner:&mut SetupState,enabled:bool)->Result<(),RemoteError>{
  let runtime=inner.runtime.as_ref().ok_or_else(unavailable)?;
  let mut db=self.state.db.lock().map_err(|_|unavailable())?;
  let mut store=RemoteStore::new(&mut db,runtime.identity().id());
  let mut preferences=store.preferences().map_err(|_|unavailable())?;
  if preferences.revision.value()!=inner.revision{return Err(RemoteError::new(ErrorCode::StaleState,"Remote settings changed. Refresh and try again."));}
  preferences.enabled=enabled;
  let saved=store.save_preferences(&preferences).map_err(|_|config_error())?;
  inner.enabled=saved.enabled;inner.revision=saved.revision.value();Ok(())
 }
 async fn reconcile_locked(&self,inner:&mut SetupState,interfaces:&[NetworkInterface],epoch:u64)->Result<(),RemoteError>{
  if !inner.enabled||inner.suspended{return Ok(());}
  let config=inner.config.as_ref().filter(|config|config.revision==inner.revision).cloned().ok_or_else(config_error)?;
  let addresses=network::selected_addresses(interfaces,&config.interface_name,config.port)?;
  if self.server.addresses().await==addresses&&inner.network_epoch==Some(epoch){return Ok(());}
  self.stop_locked(inner).await;
  let selected=BindAddresses::selected(addresses.clone(),&addresses.iter().map(SocketAddr::ip).collect::<Vec<_>>())?;
  let runtime=inner.runtime.clone().ok_or_else(unavailable)?;
  self.server.start(runtime.clone(),selected).await?;
  inner.network_epoch=Some(epoch);
  match Advertisement::start(runtime.identity().id().as_str(),self.name,&config.interface_name,&addresses){
   Ok(advertisement)=>{inner.advertisement=Some(advertisement);inner.status="Remote control is listening on the selected local network.".into();}
   Err(error)=>inner.status=error.to_string(),
  }
  Ok(())
 }
 pub async fn reconcile(&self){
  let mut inner=self.inner.lock().await;
  if !inner.enabled||inner.suspended{return;}
  let observed=network::inventory().and_then(|interfaces|network::epoch().map(|epoch|(interfaces,epoch)));
  let result=match observed{Ok((interfaces,epoch))=>self.reconcile_locked(&mut inner,&interfaces,epoch).await,Err(error)=>Err(error)};
  if let Err(error)=result{self.stop_locked(&mut inner).await;inner.status=error.to_string();}
 }
 pub async fn settings(&self)->RemoteSettingsSnapshot{
  let inner=self.inner.lock().await;
  let interfaces=network::inventory();
  let addresses=self.server.addresses().await;
  RemoteSettingsSnapshot{revision:inner.revision,enabled:inner.enabled,listening:!addresses.is_empty(),advertising:inner.advertisement.is_some(),interface_name:inner.config.as_ref().map(|c|c.interface_name.clone()),port:inner.config.as_ref().map(|c|c.port).unwrap_or(self.default_port),interfaces:interfaces.unwrap_or_default(),addresses:addresses.iter().map(ToString::to_string).collect(),requires_repairing:inner.requires_repairing,status:inner.status.clone(),pairing_attempt_id:inner.runtime.as_ref().and_then(|runtime|runtime.pairing_status().ok().flatten().map(|(id,_)|id.as_str().to_owned()))}
 }
 pub async fn set_enabled(&self,request:RemoteEnableRequest)->Result<RemoteSettingsSnapshot,RemoteError>{
  let mut inner=self.inner.lock().await;
  if request.revision!=inner.revision{return Err(RemoteError::new(ErrorCode::StaleState,"Remote settings changed. Refresh and try again."));}
  if inner.runtime.is_none(){return Err(unavailable());}
  if !request.enabled{
   self.stop_locked(&mut inner).await;inner.suspended=true;
   if let Err(error)=self.save_enabled(&mut inner,false){inner.status=error.to_string();return Err(error);}
   inner.status="Remote control is off.".into();inner.suspended=false;
  }else{
   let name=request.interface_name.ok_or_else(config_error)?;
   let interfaces=network::inventory()?;network::selected_addresses(&interfaces,&name,request.port)?;
   let epoch=network::epoch()?;
   let other=NetworkConfig::read(&self.opposite)?.map(|config|config.port).unwrap_or(self.other_default_port);
   if request.port==other{return Err(RemoteError::new(ErrorCode::InvalidRequest,"Production and development must use different remote ports. Choose another port."));}
   self.stop_locked(&mut inner).await;inner.suspended=true;
   // Persist disable before replacing an enabled configuration. Any later
   // interrupted write leaves either disabled preferences or a revision mismatch.
   if inner.enabled{if let Err(error)=self.save_enabled(&mut inner,false){inner.status=error.to_string();return Err(error);}}
   let config=NetworkConfig{version:1,revision:inner.revision.checked_add(1).ok_or_else(config_error)?,interface_name:name,port:request.port};
   if let Err(error)=config.save(&self.path){inner.status=error.to_string();return Err(error);}
   inner.config=Some(config);
   if let Err(error)=self.save_enabled(&mut inner,true){inner.status=error.to_string();return Err(error);}
   inner.suspended=false;
   if let Err(error)=self.reconcile_locked(&mut inner,&interfaces,epoch).await{self.stop_locked(&mut inner).await;inner.status=error.to_string();return Err(error);}
  }
  drop(inner);Ok(self.settings().await)
 }
 pub async fn begin_pairing(&self)->Result<RemotePairingDisplay,RemoteError>{
  let inner=self.inner.lock().await;
  let runtime=inner.runtime.as_ref().ok_or_else(unavailable)?;
  let addresses=self.server.addresses().await;
  if !inner.enabled||inner.suspended||addresses.is_empty(){return Err(RemoteError::new(ErrorCode::Unavailable,"Enable remote control on a trusted local network before pairing."));}
  // The client supplies its own interface scope for IPv6 link-local candidates.
  let candidates=addresses.iter().map(|address|address.ip().to_string()).collect::<Vec<_>>();
  let ready=runtime.begin_pairing(checked(serde_json::json!(candidates))?,checked(addresses[0].port())?)?;
  let attempt=ready.candidate.attempt_id.clone();
  let display=(||{
   let json=serde_json::to_vec(&ready.candidate).map_err(|_|unavailable())?;
   let qr=qrcode::QrCode::new(&json).map_err(|_|unavailable())?;
   let width=qr.width();let colors=qr.into_colors();
   let qr_rows=colors.chunks(width).map(|row|row.iter().map(|cell|if *cell==qrcode::Color::Dark{'1'}else{'0'}).collect()).collect();
   let expires_in_ms=runtime.pairing_status()?.ok_or_else(unavailable)?.1.as_millis() as u64;
   Ok(RemotePairingDisplay{attempt_id:attempt.as_str().to_owned(),qr_rows,addresses:candidates,port:addresses[0].port(),manual_code:ready.manual_code.as_str().to_owned(),verification_words:ready.verification_phrase.words.as_slice().iter().map(|word|word.as_str().to_owned()).collect(),expires_in_ms})
  })();
  if display.is_err(){let _=runtime.cancel_pairing(&attempt);}display
 }
 pub async fn cancel_pairing(&self,attempt:&Id)->Result<(),RemoteError>{
  let inner=self.inner.lock().await;inner.runtime.as_ref().ok_or_else(unavailable)?.cancel_pairing(attempt)
 }
 pub async fn devices(&self)->Result<Vec<RemoteDevice>,RemoteError>{
  let inner=self.inner.lock().await;
  // An initialization failure should not prevent reading the local media UI.
  let Some(runtime)=&inner.runtime else{return Ok(Vec::new());};
  runtime.list_devices()?.into_iter().map(|device|Ok(RemoteDevice{device_id:device.device_id.as_str().to_owned(),label:device.label.as_str().to_owned(),created_at_ms:serde_json::to_value(device.created_at_ms).ok().and_then(|v|v.as_u64()).ok_or_else(unavailable)?,last_seen_at_ms:device.last_seen_at_ms.and_then(|value|serde_json::to_value(value).ok().and_then(|v|v.as_u64()))})).collect()
 }
 pub async fn revoke(&self,device:&Id)->Result<(),RemoteError>{
  let inner=self.inner.lock().await;inner.runtime.as_ref().ok_or_else(unavailable)?.revoke_device(device)
 }
 pub async fn stop(&self){let mut inner=self.inner.lock().await;inner.suspended=true;self.stop_locked(&mut inner).await;}
}

#[tauri::command]
pub async fn get_remote_settings(setup:tauri::State<'_,RemoteSetup>)->Result<RemoteSettingsSnapshot,RemoteError>{Ok(setup.settings().await)}
#[tauri::command]
pub async fn set_remote_enabled(setup:tauri::State<'_,RemoteSetup>,config:RemoteEnableRequest)->Result<RemoteSettingsSnapshot,RemoteError>{setup.set_enabled(config).await}
#[tauri::command]
pub async fn begin_remote_pairing(setup:tauri::State<'_,RemoteSetup>)->Result<RemotePairingDisplay,RemoteError>{setup.begin_pairing().await}
#[tauri::command]
pub async fn cancel_remote_pairing(setup:tauri::State<'_,RemoteSetup>,attempt_id:String)->Result<(),RemoteError>{setup.cancel_pairing(&checked(attempt_id)?).await}
#[tauri::command]
pub async fn list_remote_devices(setup:tauri::State<'_,RemoteSetup>)->Result<Vec<RemoteDevice>,RemoteError>{setup.devices().await}
#[tauri::command]
pub async fn revoke_remote_device(setup:tauri::State<'_,RemoteSetup>,device_id:String)->Result<(),RemoteError>{setup.revoke(&checked(device_id)?).await}
#[cfg(test)]
mod tests {
 use super::*;
 #[test]
 fn native_config_roundtrips_atomically_and_rejects_corruption() {
  let dir=tempfile::tempdir().unwrap();let path=dir.path().join("network-v1.json");
  assert!(NetworkConfig::read(&path).unwrap().is_none());
  let config=NetworkConfig{version:1,revision:2,interface_name:"wlan0".into(),port:47840};config.save(&path).unwrap();
  let saved=NetworkConfig::read(&path).unwrap().unwrap();assert_eq!(saved.interface_name,"wlan0");assert_eq!(saved.port,47840);
  for contents in [r#"{"version":2,"revision":2,"interfaceName":"wlan0","port":47840}"#,r#"{"version":1,"revision":2,"interfaceName":"../tun0","port":0}"#,"invalid"] {
   std::fs::write(&path,contents).unwrap();assert!(NetworkConfig::read(&path).is_err());
  }
 }
}
