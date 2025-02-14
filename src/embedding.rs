use tch::{Device, Kind};
use tch::nn::{ModuleT, VarStore};
use tch::vision::imagenet;
use tch::vision::resnet::resnet34;
use crate::error::AppError;


pub fn extract_features(img: &[u8]) -> anyhow::Result<Vec<f32>, AppError> {



    let image = imagenet::load_image_and_resize_from_memory(img,224,224)?;

    let mut vs = VarStore::new(Device::Cpu);

    // Then the model is built on this variable store, and the weights are loaded.
    let resnet18 = resnet34(&vs.root(), imagenet::CLASS_COUNT);
    vs.load("D:\\dev\\tools\\resnet34.ot")?;


    let output = resnet18
        .forward_t(&image.unsqueeze(0), /*train=*/ false)
        .softmax(-1,Kind::Float);




    // Finally print the top 5 categories and their associated probabilities.
    for (probability, class) in imagenet::top(&output, 7).iter() {
        println!("{:50} {:5.2}%", class, 100.0 * probability)
    }



    let features = output.flatten(1, 1); // Flatten the output into a 1D feature vector

    let mut vec_f32: Vec<f32> = vec![0.0; features.numel()];
    features.copy_data(&mut vec_f32, features.numel());

    dbg!(&vec_f32);

    Ok(vec_f32)


}
