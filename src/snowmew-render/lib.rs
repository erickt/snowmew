 #![crate_id = "github.com/csherratt/snowmew#snowmew-render:0.1"]
#![license = "ASL2"]
#![crate_type = "lib"]
#![comment = "A game engine in rust"]
#![allow(dead_code)]

extern crate std;
extern crate glfw;
extern crate cgmath;
extern crate snowmew;
extern crate cow;
extern crate gl;
extern crate OpenCL;
extern crate ovr = "oculus-vr";
extern crate collections;
extern crate native;
extern crate time;
extern crate libc;
extern crate sync;

use std::mem;
use std::comm::{Receiver, Sender, Empty, Disconnected};

use OpenCL::hl::{CommandQueue, Context, Device};
use sync::Arc;

use snowmew::core::ObjectKey;
use snowmew::camera::Camera;
use snowmew::io::Window;
use snowmew::position::Positions;
use snowmew::graphics::Graphics;

pub use config::Config;

use pipeline::{DrawTarget, Pipeline};
use drawlist::{Drawlist, DrawlistStandard};
use time::precise_time_s;

mod db;
mod shader;
mod vertex_buffer;
mod drawlist;
mod hmd;
mod pipeline;
mod query;
mod compute_accelerator;
mod config;

trait RenderData : Graphics + Positions + Clone {}

impl RenderData for snowmew::core::Database {}

enum RenderCommand {
    Update(~RenderData:Send, ObjectKey, ObjectKey),
    Waiting(Sender<Option<DrawlistStandard>>),
    Complete(DrawlistStandard),
    Setup(Sender<Option<CommandQueue>>),
    Finish(Sender<()>)
}

fn swap_buffers(disp: &mut Window) {
    disp.swap_buffers();
    /*unsafe {
        gl::DrawElements(gl::TRIANGLES, 6i32, gl::UNSIGNED_INT, ptr::null());
        let sync = gl::FenceSync(gl::SYNC_GPU_COMMANDS_COMPLETE, 0);
        gl::ClientWaitSync(sync, gl::SYNC_FLUSH_COMMANDS_BIT, 1_000_000_000u64);
        gl::DeleteSync(sync);
    }*/
}

fn render_task(chan: Sender<RenderCommand>) {
    let (sender, receiver) = channel();
    chan.send(Setup(sender));
    let _ = receiver.recv();

    let (sender, receiver) = channel();
    chan.send(Waiting(sender.clone()));
    loop {
        for dl in receiver.iter() {
            match dl {
                Some(mut dl) => {
                    dl.setup_scene_async();
                    chan.send(Waiting(sender.clone()));
                    chan.send(Complete(dl));                    
                },
                None => {
                    println!("render task: exiting");
                    return
                }
            }

        }
    }
}

fn render_server(port: Receiver<RenderCommand>, db: ~RenderData, window: Window, size: (i32, i32),
                 _: Option<(Arc<Context>, Arc<CommandQueue>, Arc<Device>)>) {
    let (_, _, queue) = OpenCL::util::create_compute_context_prefer(OpenCL::util::GPUPrefered).unwrap();

    let mut queue = Some(queue);

    let mut db = db::GlState::new(db);
    let mut scene = 0;
    let mut camera = 0;
    let mut window = window;
    let cfg = Config::new(window.get_context_version());

    window.make_context_current();

    let mut pipeline = {
        let (width, height) = size;
        pipeline::Defered::new(pipeline::Forward::new(), width as uint, height as uint)
    };

    // todo move!
    gl::Enable(gl::SCISSOR_TEST);
    gl::Enable(gl::DEPTH_TEST);
    gl::Enable(gl::CULL_FACE);
    gl::Enable(gl::LINE_SMOOTH);
    gl::Enable(gl::BLEND);
    gl::CullFace(gl::BACK);

    db.load(&cfg);

    //let accl = PositionGlAccelerator::new();

    let mut drawlists = vec!(DrawlistStandard::from_config(&cfg, None),
                             DrawlistStandard::from_config(&cfg, None));

    let mut num_workers = 1;
    let mut waiting = Vec::new();

    let mut time = precise_time_s();

    loop {
        let cmd = if drawlists.len() == 0 || waiting.len() == 0 || scene == 0 {
            Some(port.recv())
        } else {
            match port.try_recv() {
                Err(Empty) => None,
                Err(Disconnected) => return,
                Ok(dat) => Some(dat)
            }
        };

        match cmd {
            Some(Setup(ch)) => {
                let mut out = None;
                mem::swap(&mut queue, &mut out);
                ch.send(out)
            },
            Some(Update(new, s, c)) => {
                db.update(new);
                db.load(&cfg);
                scene = s;
                camera = c;
            },
            Some(Waiting(ch)) => {
                if scene != 0 && drawlists.len() != 0 {
                    let mut dl = drawlists.pop().unwrap();
                    dl.bind_scene(db.clone(), scene);
                    ch.send(Some(dl));
                } else {
                    waiting.push(ch);
                }
            },
            Some(Complete(mut dl)) => {
                dl.setup_scene();
                let rot = db.location(camera).unwrap().get().rot;
                let camera_trans = db.position(camera);

                //let input = ih.get();
                //let rift = input.predicted;
                //et rift = rift.mul_q(&Rotation3::from_axis_angle(&Vec3::new(0f32, 1f32, 0f32), deg(180 as f32).to_rad()));

                let camera = Camera::new(rot, camera_trans);

                let dt = DrawTarget::new(0, (0, 0), (1280, 800));

                pipeline.render(&mut dl, &db, &camera.get_matrices(size), &dt);

                swap_buffers(&mut window);
                
                let end = precise_time_s();
                println!("fps: {:3.2f}", 1./(end-time));
                time = end;

                drawlists.push(dl);
            },
            Some(Finish(ack)) => {
                // flush the port, this should release any
                // async drawlist workers
                println!("render: dropping waiting");
                while waiting.len() > 0 {
                    let c = waiting.pop().unwrap();
                    c.send(None);
                    num_workers -= 1;
                }
                println!("render: waiting for open connections to close");
                while num_workers > 0 {
                    match port.recv() {
                        Waiting(ch) => {
                            num_workers -= 1;
                            ch.send(None)
                        },
                        _ => ()
                    }
                }
                ack.send(());
                println!("render: exiting");
                return;
            },
            None => {
                if drawlists.len() > 0 && waiting.len() > 0 {
                    println!("sending");
                    let ch = waiting.pop().unwrap();
                    let mut dl = drawlists.pop().unwrap();
                    dl.bind_scene(db.clone(), scene);
                    ch.send(Some(dl));
                }  
            }
        }
    }

}

pub struct RenderManager {
    ch: Sender<RenderCommand>
}

impl RenderManager {
    fn _new(db: ~RenderData:Send, window: Window, size: (i32, i32), dev: Option<Arc<Device>>) -> RenderManager {
        let (sender, receiver) = channel();

        let cl = match dev {
            Some(dev) => {
                let ctx = Arc::new(dev.create_context());
                let queue = Arc::new(ctx.create_command_queue(dev.deref()));
                Some((ctx, queue, dev))
            },
            None => None
        };

        let mut taskopts = std::task::TaskOpts::new();
        taskopts.name = Some("render-main".into_maybe_owned());

        native::task::spawn_opts(taskopts, proc() {
            let db = db;
            let window = window;

            render_server(receiver, db, window, size, cl);
        });

        let mut taskopts = std::task::TaskOpts::new();
        taskopts.name = Some("render worker #0".into_maybe_owned());

        let task_c = sender.clone();
        native::task::spawn_opts(taskopts, proc() {
            render_task(task_c);
        });

        RenderManager { ch: sender }
    }

    pub fn new_cl(db: ~RenderData:Send, window: Window, size: (i32, i32), device: Arc<Device>) -> RenderManager {
        RenderManager::_new(db, window, size, Some(device))
    }

    pub fn new(db: ~RenderData:Send, window: Window, size: (i32, i32)) -> RenderManager {
        RenderManager::_new(db, window, size, None)
    }

    pub fn update(&mut self, db: ~RenderData:Send, scene: ObjectKey, camera: ObjectKey) {
        self.ch.send(Update(db, scene, camera));
    }
}

impl Drop for RenderManager {
    fn drop(&mut self) {
        let (c, p) = channel();
        self.ch.send(Finish(c));
        let _ = self.ch;
        p.recv();
    }
}