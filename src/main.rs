mod tcp_receiver;

use gtk::gdk_pixbuf::Pixbuf;
use gtk::glib::Bytes;
use gtk::prelude::*;
use gtk::{glib, Application, ApplicationWindow, Picture};
use image::RgbImage;

use std::ops::Deref;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

fn main() {
    let app = Application::new(Some("com.github.rouhim.sensor-display"), Default::default());

    app.connect_activate(|app| {
        let window = ApplicationWindow::new(app);
        window.set_title(Some("Sensor Display"));
        window.fullscreen();

        // Show empty picture
        window.set_child(Some(&Picture::new()));

        window.present();
    });

    // Create arc mutex for image data
    let image_data_mutex: Arc<Mutex<RgbImage>> = Arc::new(Mutex::new(RgbImage::new(0, 0)));

    // Create new thread to listen for tcp messages
    let write_image_data_mutex = image_data_mutex.clone();
    thread::spawn(move || {
        let (_handler, listener) = tcp_receiver::listen();
        tcp_receiver::receive(write_image_data_mutex, listener);
    });

    // Every 100 ms update the picture with a new one
    let cloned_app = app.clone();
    glib::timeout_add_local(Duration::from_millis(100), move || {
        // Get image data from mutex
        let mutex = image_data_mutex.lock().unwrap();
        let rgb_image = mutex.deref();

        // If no image data is available, return
        if rgb_image.width() == 0 {
            return Continue(true);
        }

        // Convert image data to gtk picture
        let picture = to_gtk_picture(rgb_image);

        cloned_app
            .active_window()
            .unwrap()
            .set_child(Some(&picture));

        Continue(true)
    });

    // Holds the application until we are done with it.
    app.run();
}

fn to_gtk_picture(rgb_image: &RgbImage) -> Picture {
    let image_data = rgb_image.clone().into_raw();
    let image_bytes: Bytes = Bytes::from(&image_data);

    Picture::for_pixbuf(&Pixbuf::from_bytes(
        &image_bytes,
        gtk::gdk_pixbuf::Colorspace::Rgb,
        false,
        8,
        rgb_image.width() as i32,
        rgb_image.height() as i32,
        rgb_image.width() as i32 * 3,
    ))
}
