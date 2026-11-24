//! Code for each effect kind, translated from MuMain. Each file is one family and has
//! `pub fn register(r: &mut Registry)`, which registers its kinds.

pub mod common;
pub mod particles_1;
pub mod particles_2;
pub mod particles_3;
pub mod particles_4;
pub mod particles_5;
pub mod particles_6;
pub mod joints_1;
pub mod joints_2;
pub mod joints_3;
pub mod effects_dk1;
pub mod effects_dk2;
pub mod skills_dk;
pub mod objects_1;
pub mod objects_2;
pub mod objects_login;
pub mod objects_elbeland;
pub mod objects_kalima;
pub mod objects_kanturu;
pub mod objects_raklion;
pub mod fire;

pub mod effects_elf;
pub mod effects_summoner;
pub mod effects_wizard;
pub mod effects_dl_mg1;
pub mod effects_dl_mg2;

use super::core::Registry;

pub fn register_all(r: &mut Registry) {
    particles_1::register(r);
    particles_2::register(r);
    particles_3::register(r);
    particles_4::register(r);
    particles_5::register(r);
    particles_6::register(r);
    joints_1::register(r);
    joints_2::register(r);
    joints_3::register(r);
    effects_dk1::register(r);
    effects_dk2::register(r);
    skills_dk::register(r);
    objects_1::register(r);
    objects_2::register(r);
    objects_login::register(r);
    objects_elbeland::register(r);
    objects_kalima::register(r);
    objects_kanturu::register(r);
    objects_raklion::register(r);
    fire::register(r);
    effects_elf::register(r);
    effects_summoner::register(r);
    effects_wizard::register(r);
    effects_dl_mg1::register(r);
    effects_dl_mg2::register(r);
}
