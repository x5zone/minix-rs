//! Kernel IPC module

use minix_ipc::{Endpoint, Message};

/// 发送消息
pub fn send(dest: Endpoint, msg: &Message) {
    // TODO: 实现内核 IPC 发送
}

/// 接收消息
pub fn receive(src: Endpoint, msg: &mut Message) {
    // TODO: 实现内核 IPC 接收
}
