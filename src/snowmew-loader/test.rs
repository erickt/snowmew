extern crate loader = "snowmew-loader";

use loader::Obj;

#[test]
fn load_teapot()
{
    let teapot = Obj::load(&Path::new("assets/suzanne.obj"));

    assert!(teapot.is_some());

    let teapot = teapot.unwrap();

    for &(ref name, _, _) in teapot.objects.iter() {
        assert!(name.as_slice() == "Suzanne");
    }
}