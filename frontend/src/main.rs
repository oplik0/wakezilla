use std::collections::HashMap;
use web_sys::window;

pub mod models;
use gloo_net::http::Request;
use leptos::{leptos_dom::logging::console_log, prelude::*};
use leptos_meta::*;
use leptos_router::{
    StaticSegment,
    components::{A, Route, Router, Routes},
};
use validator::Validate;

use web_sys::{SubmitEvent, console};

// API Configuration
const API_BASE: &str = "http://localhost:3000/api";
use crate::models::{DiscoveredDevice, Machine, NetworkInterface, PortForward};

async fn create_machine(machine: Machine) -> Result<(), String> {
    Request::post(&format!("{}/machines", API_BASE))
        .json(&machine)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

async fn delete_machine(mac: &str) -> Result<(), String> {
    let payload = serde_json::json!({ "mac": mac });
    Request::delete(&format!("{}/machines/delete", API_BASE))
        .json(&payload)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

async fn fetch_machines() -> Result<Vec<Machine>, String> {
    Request::get(&format!("{}/machines", API_BASE))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

async fn fetch_interfaces() -> Result<Vec<NetworkInterface>, String> {
    Request::get(&format!("{}/interfaces", API_BASE))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}
async fn fetch_scan_network(device: String) -> Result<Vec<DiscoveredDevice>, String> {
    let mut url = String::new();
    if device.is_empty() {
        url = format!("{}/scan", API_BASE);
    } else {
        url = format!("{}/scan?interface={}", API_BASE, device);
    }
    Request::get(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

async fn is_machine_online(machine: &Machine) -> bool {
    let url = format!(
        "http://{}:{}/health",
        machine.ip,
        machine.turn_off_port.unwrap_or(3000)
    );
    let response = Request::get(&url).send().await;
    match response {
        Ok(res) => res.status() == 200,
        Err(e) => {
            console_log(&format!(
                "Network error for machine {}: {}",
                machine.name, e
            ));
            false // Mark as offline on network errors
        }
    }
}

#[component]
fn Navbar() -> impl IntoView {
    view! {
        <nav class="">
            <div class="">
                <img class="sitelogo" src="/logo_site.png" />
                <div class="nav-links">
                    <A href="/">"Sale"</A>
                    <A href="/transactions">"Transactions"</A>
                    <A href="/items">"Items"</A>
                    <A href="/categories">"Categories"</A>
                    <A href="/reports">"Reports"</A>
                </div>
            </div>
        </nav>
    }
}

#[component]
pub fn ErrorDisplay(
    erros: ReadSignal<HashMap<String, Vec<String>>>,
    key: &'static str,
) -> impl IntoView {
    view! {
        {move || {
            if erros.get().contains_key(key) {
                let error_messages = erros.get().get(key).cloned().unwrap_or_default();
                Some(
                    view! {
                        <div class="error-message">
                            <For
                                each=move || error_messages.clone().into_iter()
                                key=|msg| msg.clone()
                                children=move |msg| {
                                    view! { <p>{msg}</p> }
                                }
                            />
                        </div>
                    },
                )
            } else {
                None
            }
        }}
    }
}
// Components
#[component]
fn App() -> impl IntoView {
    provide_meta_context();

    view! {
        <Html attr:lang="en" />
        <Stylesheet id="leptos" href="/style/main.css" />
        <Title text="Wakezilla" />
        <Router>
            <main class="container">
                <Routes fallback=|| "Page not found">
                    <Route path=StaticSegment("") view=HomePage />
                </Routes>
            </main>
        </Router>
    }
}

#[component]
fn Header(set_machine: WriteSignal<Machine>) -> impl IntoView {
    let (discovered_devices, set_discovered_devices) = signal::<Vec<DiscoveredDevice>>(vec![]);
    let (interfaces, set_interfaces) = signal::<Vec<NetworkInterface>>(vec![]);
    let (interface, set_interface) = signal::<String>("".to_string());
    let (loading, set_loading) = signal(false);

    // Load initial data
    Effect::new(move || {
        leptos::task::spawn_local(async move {
            if let Ok(cats) = fetch_interfaces().await {
                set_interfaces.set(cats);
            }
        });
    });

    fn handle_interface_change(value: String, set_interface: WriteSignal<String>) {
        let log_mesasge = format!("Selected interface: {}", value);
        console_log(&log_mesasge);
        set_interface.set(value);
    }

    let on_submit = move |ev: SubmitEvent| {
        let set_loading = set_loading.clone();
        set_loading.set(true);
        set_discovered_devices.set(vec![]);
        // stop the page from reloading!
        ev.prevent_default();
        console::log_1(&format!("Form submitted with value: {}", interface.get()).into());
        leptos::task::spawn_local(async move {
            fetch_scan_network(interface.get())
                .await
                .map(|devices| {
                    console::log_1(&format!("Discovered devices: {:?}", devices).into());
                    set_discovered_devices.set(devices);
                })
                .unwrap_or_else(|err| {
                    window()
                        .unwrap()
                        .alert_with_message(&format!(
                            "Error scanning network, check the logs in the backend"
                        ))
                        .unwrap();
                    console::log_1(&format!("Error scanning network: {}", err).into());
                });

            set_loading.set(false);
        });
    };

    fn handle_add_machine(
        device: DiscoveredDevice,
        set_machine: WriteSignal<Machine>,
        set_discovered_devices: WriteSignal<Vec<DiscoveredDevice>>,
    ) {
        let new_machine = Machine {
            name: device.hostname.clone().unwrap_or_else(|| "".to_string()),
            mac: device.mac.clone(),
            ip: device.ip.clone(),
            description: None,
            turn_off_port: None,
            can_be_turned_off: false,
            port_forwards: vec![],
        };
        set_machine.set(new_machine);
        set_discovered_devices.set(vec![]);
    }

    view! {
        <div class="">
            <div>
                <h1 class="">Wakezilla Manager</h1>
                <p class="">Wake, manage, and forward to your registered machines.</p>
            </div>
            <form
                on:submit=on_submit
                class=""
                style="margin-top: 1rem; margin-bottom: 1rem; display: flex; gap: 0.5rem; align-items: center;"
            >
                <select
                    id="interface-select"
                    class=""
                    on:change:target=move |ev| {
                        handle_interface_change(ev.target().value(), set_interface);
                    }
                    prop:value=move || interface.get().to_string()
                >
                    <option value="">Auto-detect interface</option>
                    {move || {
                        interfaces
                            .get()
                            .iter()
                            .map(|iface| {
                                view! {
                                    <option value=iface
                                        .name
                                        .clone()>
                                        {format!("{} - {} ({})", iface.name, iface.ip, iface.mac)}
                                    </option>
                                }
                            })
                            .collect::<Vec<_>>()
                    }}
                </select>
                <button id="scan-btn" class="" disabled=move || loading.get()>
                    {move || {
                        if loading.get() { "🔍 Scanning  ...." } else { "🔍 Scan Network" }
                    }}
                </button>
            </form>

        </div>
        <Show when=move || { !discovered_devices.get().is_empty() } fallback=|| view! { "" }>

            <section id="scan-results-container">
                <div class="">
                    <h3 style="eargin-top: 1rem; margin-bottom: 1rem;">Discovered Devices</h3>
                    <span id="scan-status" class="" aria-live="polite"></span>
                </div>
                <div class="">
                    <table id="scan-results-table" style="width: 100%;">
                        <thead class="">
                            <tr>
                                <th class="">IP Address</th>
                                <th class="">Hostname</th>
                                <th class="">MAC Address</th>
                                <th class="">Action</th>
                            </tr>
                        </thead>
                        <tbody class="">
                            <For
                                each=move || discovered_devices.get()
                                key=|device| device.ip.clone()
                                children=move |device| {
                                    view! {
                                        <tr>
                                            <td class="">{device.ip.clone()}</td>
                                            <td class="">
                                                {device
                                                    .hostname
                                                    .clone()
                                                    .unwrap_or_else(|| "N/A".to_string())}
                                            </td>
                                            <td class="">{device.mac.clone()}</td>
                                            <td class="px-4 py-3">
                                                <button on:click=move |_| {
                                                    handle_add_machine(
                                                        device.clone(),
                                                        set_machine.clone(),
                                                        set_discovered_devices.clone(),
                                                    );
                                                }>{"＋"}</button>
                                            </td>
                                        </tr>
                                    }
                                }
                            />
                        </tbody>
                    </table>
                </div>
            </section>
        </Show>
    }
}

#[component]
fn RegistredMachines(
    machines: ReadSignal<Vec<Machine>>,
    status_machine: ReadSignal<HashMap<String, bool>>,
    set_registred_machines: WriteSignal<Vec<Machine>>,
) -> impl IntoView {
    let on_delete = move |mac_to_delete: String| {
        leptos::task::spawn_local(async move {
            // Call the API to delete the machine
            if let Err(err) = delete_machine(&mac_to_delete).await {
                console_log(&format!(
                    "Error deleting machine {}: {}",
                    mac_to_delete, err
                ));
                window()
                    .unwrap()
                    .alert_with_message(&format!("Error deleting machine: {}", err))
                    .unwrap();
                return;
            }

            // Remove the machine from the local state
            let current_machines = machines.get();
            let filtered_machines: Vec<Machine> = current_machines
                .into_iter()
                .filter(|m| m.mac != mac_to_delete)
                .collect();

            set_registred_machines.set(filtered_machines);
            console_log(&format!("Successfully deleted machine: {}", mac_to_delete));
        });
    };

    view! {
        <section class="" style="width: 100%; margin-top: 2rem;">
            <div
                class=""
                style="width: 100%; margin-bottom: 0.75rem; display: flex; align-items: center; justify-content: space-between;"
            >
                <h2 class="" style="font-size: 1.125rem; font-weight: 600;">
                    Registered Machines
                </h2>
            </div>
            <div
                class=""
                style="width: 100%; overflow-x: auto; border-radius: 0.5rem; border: 1px solid #e5e7eb; background-color: white; box-shadow: 0 1px 2px 0 rgba(0, 0, 0, 0.05); display: block;"
            >
                <table
                    class=""
                    style="width: 100%; min-width: 100%; text-align: left; font-size: 0.875rem;"
                >
                    <thead class="" style="background-color: #f9fafb; color: #374151;">
                        <tr>
                            <th
                                class="px-4 py-3 font-semibold"
                                style="padding: 0.75rem; font-weight: 600;"
                            >
                                Name
                            </th>
                            <th
                                class="px-4 py-3 font-semibold"
                                style="padding: 0.75rem; font-weight: 600;"
                            >
                                MAC Address
                            </th>
                            <th
                                class="px-4 py-3 font-semibold"
                                style="padding: 0.75rem; font-weight: 600;"
                            >
                                IP Address
                            </th>
                            <th
                                class="px-4 py-3 font-semibold"
                                style="padding: 0.75rem; font-weight: 600;"
                            >
                                Description
                            </th>
                            <th
                                class="px-4 py-3 font-semibold"
                                style="padding: 0.75rem; font-weight: 600;"
                            >
                                Status
                            </th>
                            <th
                                class="px-4 py-3 font-semibold"
                                style="padding: 0.75rem; font-weight: 600;"
                            >
                                Actions
                            </th>
                        </tr>
                    </thead>
                    <tbody class="" style="border-top: 1px solid #e5e7eb;">
                        {move || {
                            machines
                                .get()
                                .iter()
                                .map(|m| {
                                    let machine = m.clone();
                                    // Clone the machine for the closure
                                    view! {
                                        <tr
                                            class=""
                                            style="vertical-align: middle; border-bottom: 1px solid #e5e7eb;"
                                        >
                                            <td class="" style="padding: 0.75rem; font-size: 0.75rem;">
                                                <a
                                                    class=""
                                                    href="/machines/{ machine.mac }"
                                                    style="text-decoration: underline; color: #2563eb; display: block; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;"
                                                >
                                                    {machine.name.clone()}
                                                </a>
                                            </td>
                                            <td
                                                class=""
                                                style="padding: 0.75rem; font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace; font-size: 0.75rem;"
                                            >
                                                {machine.mac.clone()}
                                            </td>
                                            <td
                                                class=""
                                                style="padding: 0.75rem; font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace; font-size: 0.75rem;"
                                            >
                                                {machine.ip.clone()}
                                            </td>
                                            <td class="" style="padding: 0.75rem;">
                                                <span class="" style="font-size: 0.75rem;">
                                                    {machine
                                                        .description
                                                        .clone()
                                                        .unwrap_or_else(|| "-".to_string())}
                                                </span>
                                            </td>
                                            <td class="" style="padding: 0.75rem;">
                                                <span
                                                    class=""
                                                    style="font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace; font-size: 0.75rem;"
                                                >
                                                    {status_machine
                                                        .get()
                                                        .get(&machine.mac)
                                                        .cloned()
                                                        .unwrap_or(false)
                                                        .then(|| {
                                                            view! { <span style="color: green;">"Online"</span> }
                                                        })
                                                        .unwrap_or_else(|| {
                                                            view! { <span style="color: red;">"Offline"</span> }
                                                        })}
                                                </span>
                                            </td>
                                            <td class="" style="padding: 0.75rem;">
                                                <button
                                                    on:click=move |_| {
                                                        if window()
                                                            .unwrap()
                                                            .confirm_with_message(
                                                                &format!(
                                                                    "Are you sure you want to delete machine {}?",
                                                                    machine.name,
                                                                ),
                                                            )
                                                            .unwrap_or(false)
                                                        {
                                                            on_delete(machine.mac.clone());
                                                        }
                                                    }
                                                    style="color: #dc2626; background: none; border: none; cursor: pointer; font-size: 1rem;"
                                                >
                                                    "🗑️"
                                                </button>
                                            </td>
                                        </tr>
                                    }
                                })
                                .collect::<Vec<_>>()
                        }}
                    </tbody>
                </table>
            </div>
        </section>
    }
}

#[component]
fn AddMachine(
    machine: ReadSignal<Machine>,
    registred_machines: ReadSignal<Vec<Machine>>,
    set_registred_machines: WriteSignal<Vec<Machine>>,
) -> impl IntoView {
    let (machine_form_data, set_machine_form_data) = signal::<Machine>(machine.get());
    let (erros, set_errors) = signal::<HashMap<String, Vec<String>>>(HashMap::new());
    let (loading, set_loading) = signal(false);
    Effect::new(move |_| {
        set_machine_form_data.set(machine.get());
    });

    fn set_input_value(
        key: &str,
        value: String,
        set_machine_form_data: WriteSignal<Machine>,
        machine_form_data: ReadSignal<Machine>,
    ) {
        //console_log(&format!("Setting {} to {}", key, value));
        let mut current = machine_form_data.get();
        match key {
            "name" => current.name = value,
            "mac" => current.mac = value,
            "ip" => current.ip = value,
            "description" => current.description = Some(value),
            "turn_off_port" => {
                current.turn_off_port = value.parse().ok();
            }
            "can_be_turned_off" => {
                current.can_be_turned_off = value == "on";
            }
            _ => {}
        };
        set_machine_form_data.set(current);
    }

    let on_submit = move |ev: SubmitEvent| {
        // stop the page from reloading!
        ev.prevent_default();
        set_loading.set(true);
        match machine_form_data.get().validate() {
            Ok(_) => {
                console::log_1(&"Form is valid".into());
                let current_machines = registred_machines.get();
                let mut new_machines = current_machines.clone();

                leptos::task::spawn_local(async move {
                    if let Ok(_) = create_machine(machine_form_data.get()).await {
                        //console_log(&format!("Loaded {:?} machines", machines));
                        new_machines.insert(0, machine_form_data.get());

                        set_registred_machines.set(new_machines);
                        // Clear the form
                        set_machine_form_data.set(Machine {
                            name: "".to_string(),
                            mac: "".to_string(),
                            ip: "".to_string(),
                            description: None,
                            turn_off_port: None,
                            can_be_turned_off: false,
                            port_forwards: vec![],
                        });
                        set_errors.set(HashMap::new());
                    } else {
                        console_log(&"Error creating machine".to_string());
                    }
                });
                set_loading.set(false);
            }
            Err(e) => {
                let mut new_errors = HashMap::new();
                for (field, errors) in e.field_errors() {
                    let mut field_errors = vec![];
                    for error in errors {
                        field_errors.push(error.code.to_string());
                    }
                    new_errors.insert(field.to_string(), field_errors);
                }
                set_errors.set(new_errors);
                return;
            }
        }
    };

    view! {
        <section style="margin-top: 2rem; margin-bottom: 2rem;">
            <div class="">
                <h3 class="">Add New Machine {move || machine_form_data.get().ip}</h3>
            </div>
            <form on:submit=on_submit class="">
                <div>
                    <div class="form-fields">
                        <label for="name" class="">
                            "Name"
                        </label>
                        <input
                            type="text"
                            id="name"
                            name="name"
                            required
                            class=""
                            on:input:target=move |ev| {
                                let input_value = ev.target().value();
                                set_input_value(
                                    "name",
                                    input_value,
                                    set_machine_form_data.clone(),
                                    machine_form_data.clone(),
                                );
                            }
                            prop:value=move || machine_form_data.get().name
                        />
                    </div>

                    <ErrorDisplay erros=erros key="name" />
                </div>
                <div>
                    <div class="form-fields">
                        <label for="mac" class="">
                            "MAC Address"
                        </label>
                        <input
                            type="text"
                            id="mac"
                            name="mac"
                            required
                            class=""
                            on:input:target=move |ev| {
                                let input_value = ev.target().value();
                                set_input_value(
                                    "mac",
                                    input_value,
                                    set_machine_form_data.clone(),
                                    machine_form_data.clone(),
                                );
                            }
                            prop:value=move || machine_form_data.get().mac
                        />

                    </div>
                    <ErrorDisplay erros=erros key="mac" />
                </div>
                <div>
                    <div class="form-fields">
                        <label for="ip" class="">
                            "IP Address"
                        </label>
                        <input
                            required
                            on:input:target=move |ev| {
                                let input_value = ev.target().value();
                                set_input_value(
                                    "ip",
                                    input_value,
                                    set_machine_form_data.clone(),
                                    machine_form_data.clone(),
                                );
                            }
                            prop:value=move || machine_form_data.get().ip
                            type="text"
                            id="ip"
                            name="ip"
                            class=""
                        />

                    </div>
                    <ErrorDisplay erros=erros key="ip" />
                </div>
                <div>
                    <div class="form-fields">
                        <label for="description" class="">
                            "Description (optional)"
                        </label>
                        <input
                            id="description"
                            on:input:target=move |ev| {
                                let input_value = ev.target().value();
                                set_input_value(
                                    "description",
                                    input_value,
                                    set_machine_form_data.clone(),
                                    machine_form_data.clone(),
                                );
                            }

                            prop:value=move || {
                                machine_form_data.get().description.clone().unwrap_or_default()
                            }
                            name="description"
                            class=""
                        />
                    </div>
                    <ErrorDisplay erros=erros key="description" />
                </div>
                <div>
                    <div class="form-fields">
                        <label for="turn_off_port" class="">
                            "Turn Off Port (optional)"
                        </label>
                        <input
                            type="number"
                            on:input:target=move |ev| {
                                let input_value = ev.target().value();
                                set_input_value(
                                    "turn_off_port",
                                    input_value,
                                    set_machine_form_data.clone(),
                                    machine_form_data.clone(),
                                );
                            }

                            prop:value=move || {
                                machine_form_data
                                    .get()
                                    .turn_off_port
                                    .clone()
                                    .unwrap_or(3000)
                                    .to_string()
                            }
                            id="turn_off_port"
                            name="turn_off_port"
                            class=""
                        />
                        <ErrorDisplay erros=erros key="turn_off_port" />
                    </div>
                </div>
                <div class="form-fields">
                    <label for="can_be_turned_off" class="">
                        "Can be turned off"
                    </label>
                    <input
                        type="checkbox"
                        on:input:target=move |ev| {
                            let input_value = if ev.target().checked() { "on" } else { "off" }
                                .to_string();
                            set_input_value(
                                "can_be_turned_off",
                                input_value,
                                set_machine_form_data.clone(),
                                machine_form_data.clone(),
                            );
                        }
                        prop:checked=move || machine_form_data.get().can_be_turned_off
                        id="can_be_turned_off"
                        name="can_be_turned_off"
                        class=""
                    />
                </div>
                <div style="font-size:21px; display: flex;justify-content: center;">

                    <button
                        type="submit"
                        disabled=move || loading.get()
                        class="submit-button submit-button:hover"
                    >
                        {move || {
                            if loading.get() { "Adding Machine..." } else { "Add Machine" }
                        }}

                    </button>
                </div>
            </form>
        </section>
    }
}
#[component]
fn HomePage() -> impl IntoView {
    let default_machine = Machine {
        name: "".to_string(),
        mac: "".to_string(),
        ip: "".to_string(),
        description: None,
        turn_off_port: None,
        can_be_turned_off: false,
        port_forwards: vec![],
    };
    let (machine, set_machine) = signal::<Machine>(default_machine);

    let (registred_machines, set_registred_machines) = signal::<Vec<Machine>>(vec![]);
    let (status_machine, set_status_machine) = signal::<HashMap<String, bool>>(HashMap::new());

    // Load initial registred machines
    Effect::new(move || {
        leptos::task::spawn_local(async move {
            if let Ok(machines) = fetch_machines().await {
                //console_log(&format!("Loaded {:?} machines", machines));
                set_registred_machines.set(machines);
            }
        });
    });

    // check the status of registred machines when they change
    Effect::new(move |_| {
        let machines = registred_machines.get();
        if machines.is_empty() {
            console_log("No registred machines");
            return;
        }

        // Spawn the async task to check all machines
        leptos::task::spawn_local(async move {
            // Create a vector of futures to check each machine concurrently
            let mut futures = Vec::new();

            for m in machines {
                let machine_mac = m.mac.clone();
                let machine_name = m.name.clone();

                console_log(&format!("Checking machine {}", machine_name));
                let future = async move { (machine_mac, is_machine_online(&m).await) };
                futures.push(future);
            }

            // Wait for all futures to complete, regardless of individual failures
            let results = futures::future::join_all(futures).await;

            // Build the status map from all results
            let mut status_map = HashMap::new();
            for (mac, is_online) in results {
                status_map.insert(mac, is_online);
            }
            set_status_machine.set(status_map);
        });
    });

    view! {
        <Header set_machine=set_machine />
        <RegistredMachines
            machines=registred_machines
            status_machine=status_machine
            set_registred_machines=set_registred_machines
        />
        <AddMachine
            machine=machine
            registred_machines=registred_machines
            set_registred_machines=set_registred_machines
        />
    }
}

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(App)
}
