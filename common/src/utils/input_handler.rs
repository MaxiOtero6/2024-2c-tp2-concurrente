use std::env;
use regex::Regex;

fn main() {
    // Obtener los argumentos pasados por línea de comandos
    let args: Vec<String> = env::args().skip(1).collect();

    // Concatenar los argumentos para procesarlos como un solo comando
    let command = args.join(" ");

    // Regex para validar el formato del comando
    let command_pattern = Regex::new(
        r"^id=(\d+)\s+origin=\((-?\d+),(-?\d+)\)\s+dest=\((-?\d+),(-?\d+)\)$"
    ).expect("Regex no válida");

    // Validar el comando
    if let Some(captures) = command_pattern.captures(&command) {
        let id: i32 = captures[1].parse().expect("ID no es un número válido");
        let origin_x: i32 = captures[2].parse().expect("Origin X no es un número válido");
        let origin_y: i32 = captures[3].parse().expect("Origin Y no es un número válido");
        let destination_x: i32 = captures[4].parse().expect("Destination X no es un número válido");
        let destination_y: i32 = captures[5].parse().expect("Destination Y no es un número válido");


        // Validar que las coordenadas no sean negativas
        if origin_x < 0 || origin_y < 0 || destination_x < 0 || destination_y < 0 {
            eprintln!("Error: Las coordenadas no pueden ser valores negativos.");
            eprintln!("Formato esperado: id=<número> origin=(x,y) destination=(x,y)");
            return;
        }

        println!("Datos válidos:");
        println!("ID: {}", id);
        println!("Origin: ({}, {})", origin_x, origin_y);
        println!("Destination: ({}, {})", destination_x, destination_y);
    } else {
        eprintln!("Error: El comando no tiene el formato correcto.");
        eprintln!("Formato esperado: id=<número> origin=(x,y) destination=(x,y)");
    }
}
