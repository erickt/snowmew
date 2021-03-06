use std::ptr;
use std::vec::Vec;
use std::str;
use std::mem;

use gl;
use gl::types::GLuint;

pub static MATRIX_PROJECTION: i32 = 0;
pub static MATRIX_MODEL: i32 = 1;

pub static ATTR_POISTION: i32 = 0;
pub static ATTR_TEXTURE: i32 = 1;
pub static ATTR_NORMAL: i32 = 2;

pub fn compile_shader(header: Option<&str>, src: &str, ty: gl::types::GLenum) -> GLuint {
    let shader = gl::CreateShader(ty);
    unsafe {
        match header {
            Some(header) => {
                header.with_c_str(|header_ptr| {
                src.with_c_str(|ptr| {
                    let s_ptrs = &[header_ptr, ptr];
                    gl::ShaderSource(shader, 2, &s_ptrs[0], ptr::null());
                })});    
            }
            None => {
                src.with_c_str(|ptr| {
                    gl::ShaderSource(shader, 1, &ptr, ptr::null());
                });                
            }
        }


        gl::CompileShader(shader);

        let mut status = gl::FALSE as gl::types::GLint;
        gl::GetShaderiv(shader, gl::COMPILE_STATUS, &mut status);

        // Fail on error
        let mut len = 0;
        gl::GetShaderiv(shader, gl::INFO_LOG_LENGTH, &mut len);

        if len != 0 {
            let mut buf = Vec::from_elem(len as uint, 0u8);     // subtract 1 to skip the trailing null character
            gl::GetShaderInfoLog(shader,
                                 len,
                                 ptr::mut_null(),
                                 mem::transmute(buf.as_mut_slice().unsafe_mut_ref(0)));
            if status == gl::FALSE as i32 {
                fail!("glsl error: {:s} {:s}", src, str::raw::from_utf8(buf.as_slice()));
            } else {
                println!("shader log {:}", str::raw::from_utf8(buf.as_slice()));
            }
        }
    }

    shader
}

#[deriving(Clone, Default)] 
pub struct Shader {
    program: GLuint,
    shaders: Vec<GLuint>
}

impl Shader {
    fn _new(shaders: Vec<GLuint>, bind_attr: &[(u32, &str)], bind_frag: &[(u32, &str)]) -> Shader {
        let program = gl::CreateProgram();
        for s in shaders.iter() {
            gl::AttachShader(program, *s);
        }
 
        unsafe {
            for &(idx, name) in bind_attr.iter() {
                name.with_c_str(|ptr| gl::BindAttribLocation(program, idx, ptr));
            }
            for &(idx, name) in bind_frag.iter() {
                name.with_c_str(|ptr| gl::BindFragDataLocation(program, idx, ptr));
            }
        }

        gl::LinkProgram(program);

        unsafe {
            let mut status = gl::FALSE as gl::types::GLint;
            gl::GetProgramiv(program, gl::LINK_STATUS, &mut status);

            // Fail on error
            if status != (gl::TRUE as gl::types::GLint) {
                let mut len = 0;
                gl::GetProgramiv(program, gl::INFO_LOG_LENGTH, &mut len);
                let mut buf = Vec::from_elem(len as uint, 0u8);     // subtract 1 to skip the trailing null character
                gl::GetProgramInfoLog(program,
                                      len,
                                      ptr::mut_null(),
                                      mem::transmute(buf.as_mut_slice().unsafe_mut_ref(0)));
                fail!("glsl error: {:s}", str::raw::from_utf8(buf.as_slice()));
            }
        }


        Shader {
            program: program,
            shaders: shaders
        }
    }

    pub fn new(vert: &str,
               frag: &str,
               bind_attr: &[(u32, &str)],
               bind_frag: &[(u32, &str)],
               header: Option<&str>) -> Shader {
        let vert = compile_shader(header, vert, gl::VERTEX_SHADER);
        let frag = compile_shader(header, frag, gl::FRAGMENT_SHADER);
        Shader::_new(vec!(vert, frag), bind_attr, bind_frag)
    }

    pub fn new_geo(vert: &str,
                   frag: &str,
                   geo: &str,
                   bind_attr: &[(u32, &str)],
                   bind_frag: &[(u32, &str)],
                   header: Option<&str>) -> Shader {
        let vert = compile_shader(header, vert, gl::VERTEX_SHADER);
        let frag = compile_shader(header, frag, gl::FRAGMENT_SHADER);
        let geo = compile_shader(header, geo, gl::GEOMETRY_SHADER);
        Shader::_new(vec!(vert, geo, frag), bind_attr, bind_frag)
    }

    pub fn compute(cs: &str, header: Option<&str>) -> Shader {
        let cs = compile_shader(header, cs, gl::COMPUTE_SHADER);
        Shader::_new(vec!(cs), &[], &[])
    }

    pub fn uniform(&self, s: &str) -> i32 {
        unsafe {
            s.with_c_str(|c_str| {
                gl::GetUniformLocation(self.program, c_str)
            })
        }
    }

    pub fn uniform_block_index(&self, s: &str) -> u32 {
        unsafe {
            s.with_c_str(|c_str| {
                gl::GetUniformBlockIndex(self.program, c_str)
            })
        }
    }

    pub fn get_program_resouce_location(&self, gltype: u32, s: &str) -> u32 {
        unsafe {
            s.with_c_str(|c_str| {
                gl::GetProgramResourceLocation(self.program, gltype, c_str) as u32
            })
        }
    }

    pub fn uniform_block_data_size(&self, idx: u32) -> i32 {
        unsafe {
            let mut val = 0;
            gl::GetActiveUniformBlockiv(self.program, idx, gl::UNIFORM_BLOCK_DATA_SIZE, &mut val);
            val
        }
    }

    pub fn uniform_block_offset(&self, idx: u32) -> i32 {
        unsafe {
            let mut val = 0;
            gl::GetActiveUniformBlockiv(self.program, idx, gl::UNIFORM_OFFSET, &mut val);
            val
        }
    }

    pub fn uniform_block_bind(&self, idx: u32, buffer: u32) {
        gl::UniformBlockBinding(self.program, idx, buffer);
    }

    pub fn bind(&self) {
        gl::UseProgram(self.program);
    }

    pub fn validate(&self)  {
        gl::ValidateProgram(self.program);
        unsafe {
            let mut status = gl::FALSE as gl::types::GLint;
            gl::GetProgramiv(self.program, gl::VALIDATE_STATUS, &mut status);

            // Fail on error
            if status != (gl::TRUE as gl::types::GLint) {
                let mut len = 0;
                gl::GetProgramiv(self.program, gl::INFO_LOG_LENGTH, &mut len);
                if len > 0 {
                    let mut buf = Vec::from_elem(len as uint, 0u8);     // subtract 1 to skip the trailing null character
                    gl::GetProgramInfoLog(self.program,
                                          len,
                                          ptr::mut_null(),
                                          mem::transmute(buf.as_mut_slice().unsafe_mut_ref(0)));
                    fail!("glsl error: {:s}", str::raw::from_utf8(buf.as_slice()));
                }
            }
        }
    }
}