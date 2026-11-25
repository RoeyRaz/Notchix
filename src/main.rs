#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(windows)]
mod app;
#[cfg(windows)]
mod glass_capture;
#[cfg(windows)]
mod notion;
#[cfg(windows)]
mod spotify;
#[cfg(windows)]
mod bar;
#[cfg(windows)]
mod dock;
#[cfg(windows)]
mod presentmon;
#[cfg(windows)]
mod game_strip;
#[cfg(windows)]
mod buddy;
mod cal;
mod cast;
mod game;
mod games;
mod skins;
mod pixdata;
#[cfg(windows)]
mod scene;
#[cfg(windows)]
mod shelf;
mod gfx;
#[cfg(windows)]
mod pop;
#[cfg(windows)]
mod media;
#[cfg(windows)]
mod mirror;
#[cfg(windows)]
mod transfer;
#[cfg(windows)]
mod explorer_copy;
#[cfg(windows)]
mod qbit;
mod prism;
#[cfg(windows)]
mod weather;
#[cfg(windows)]
mod win_notifications;
#[cfg(windows)]
mod lyrics;
#[cfg(windows)]
mod radio;
mod store;
#[cfg(windows)]
mod sys;
#[cfg(windows)]
mod ui;
#[cfg(not(windows))]
#[path = "sys_linux.rs"]
mod sys;
#[cfg(windows)]
mod gfx_win;
#[cfg(not(windows))]
mod gfx_linux;

#[cfg(windows)]
include!("win_main.rs");

#[cfg(not(windows))]
mod linux_main;
#[cfg(not(windows))]
mod media_linux;
#[cfg(not(windows))]
mod linux_pop;

#[cfg(not(windows))]
fn main() {
    linux_main::run();
}
