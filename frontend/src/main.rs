use std::collections::HashMap;

use chrono::{DateTime, Utc};
use gloo_net::http::Request;
use leptos::{leptos_dom::logging::console_log, prelude::*};
use leptos_meta::*;
use leptos_router::{
    StaticSegment,
    components::{A, Route, Router, Routes},
};
use validator::Validate;

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use web_sys::{SubmitEvent, console};

// API Configuration
const API_BASE: &str = "http://localhost:3000/api";
const CURRENCY_SYMBOL: &str = "€";

// Shared Models (matching backend)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Category {
    id: Uuid,
    name: String,
    description: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Item {
    id: Uuid,
    name: String,
    description: Option<String>,
    price: f64,
    category_id: Uuid,
    sku: Option<String>,
    in_stock: bool,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Transaction {
    id: Uuid,
    customer_name: Option<String>,
    status: String,
    total: f64,
    paid_amount: Option<f64>,
    change_amount: Option<f64>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    closed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TransactionItemDetail {
    id: Uuid,
    item_id: Uuid,
    item_name: String,
    quantity: i32,
    unit_price: f64,
    total_price: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TransactionDetailsResponse {
    transaction: Transaction,
    items: Vec<TransactionItemDetail>,
}

// Report Models
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ItemSalesReport {
    item_id: Uuid,
    item_name: String,
    category_name: String,
    quantity_sold: i64,
    total_revenue: f64,
    average_price: f64,
    transaction_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReportSummary {
    total_revenue: f64,
    total_items_sold: i64,
    total_transactions: i64,
    average_transaction_value: f64,
    top_selling_item: Option<String>,
    top_revenue_item: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SalesReport {
    start_date: DateTime<Utc>,
    end_date: DateTime<Utc>,
    items: Vec<ItemSalesReport>,
    summary: ReportSummary,
}

#[derive(Debug, Serialize)]
struct ReportDateRange {
    start_date: DateTime<Utc>,
    end_date: DateTime<Utc>,
}

// DTOs
#[derive(Debug, Serialize)]
struct CreateCategoryDto {
    name: String,
    description: Option<String>,
}

#[derive(Debug, Serialize)]
struct UpdateCategoryDto {
    name: Option<String>,
    description: Option<String>,
}

#[derive(Debug, Serialize)]
struct CreateItemDto {
    name: String,
    description: Option<String>,
    price: f64,
    category_id: Uuid,
    sku: Option<String>,
    in_stock: Option<bool>,
}

#[derive(Debug, Serialize)]
struct UpdateItemDto {
    name: Option<String>,
    description: Option<String>,
    price: Option<f64>,
    category_id: Option<Uuid>,
    sku: Option<String>,
    in_stock: Option<bool>,
}

#[derive(Debug, Serialize)]
struct CreateTransactionDto {
    customer_name: Option<String>,
}

#[derive(Debug, Serialize)]
struct AddTransactionItemDto {
    item_id: Uuid,
    quantity: i32,
}

#[derive(Debug, Serialize)]
struct UpdateTransactionDto {
    customer_name: Option<String>,
}

#[derive(Debug, Serialize)]
struct UpdateTransactionItemDto {
    item_id: Uuid,
    quantity: i32,
}

#[derive(Debug, Serialize)]
struct CloseTransactionDto {
    paid_amount: f64,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct CloseTransactionResponse {
    transaction: Transaction,
    change_amount: f64,
}

// API Client - Categories
async fn fetch_categories() -> Result<Vec<Category>, String> {
    Request::get(&format!("{}/categories", API_BASE))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

async fn create_category(dto: CreateCategoryDto) -> Result<Category, String> {
    Request::post(&format!("{}/categories", API_BASE))
        .json(&dto)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

async fn update_category(id: Uuid, dto: UpdateCategoryDto) -> Result<Category, String> {
    Request::put(&format!("{}/categories/{}", API_BASE, id))
        .json(&dto)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

async fn delete_category(id: Uuid) -> Result<(), String> {
    Request::delete(&format!("{}/categories/{}", API_BASE, id))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

// API Client - Items
async fn fetch_items() -> Result<Vec<Item>, String> {
    Request::get(&format!("{}/items", API_BASE))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

async fn create_item(dto: CreateItemDto) -> Result<Item, String> {
    Request::post(&format!("{}/items", API_BASE))
        .json(&dto)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

async fn update_item(id: Uuid, dto: UpdateItemDto) -> Result<Item, String> {
    Request::put(&format!("{}/items/{}", API_BASE, id))
        .json(&dto)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

async fn delete_item(id: Uuid) -> Result<(), String> {
    Request::delete(&format!("{}/items/{}", API_BASE, id))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

// API Client - Transactions
async fn fetch_all_transactions() -> Result<Vec<Transaction>, String> {
    Request::get(&format!("{}/transactions", API_BASE))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

async fn fetch_open_transactions() -> Result<Vec<Transaction>, String> {
    Request::get(&format!("{}/transactions/open", API_BASE))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

async fn fetch_transaction_details(id: Uuid) -> Result<TransactionDetailsResponse, String> {
    Request::get(&format!("{}/transactions/{}", API_BASE, id))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

async fn create_transaction(customer_name: Option<String>) -> Result<Transaction, String> {
    Request::post(&format!("{}/transactions", API_BASE))
        .json(&CreateTransactionDto { customer_name })
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

async fn update_transaction(
    id: Uuid,
    customer_name: Option<String>,
) -> Result<Transaction, String> {
    Request::put(&format!("{}/transactions/{}", API_BASE, id))
        .json(&UpdateTransactionDto { customer_name })
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

async fn add_item_to_transaction(
    transaction_id: Uuid,
    item_id: Uuid,
    quantity: i32,
) -> Result<(), String> {
    // Fetch current transaction details
    let details = fetch_transaction_details(transaction_id)
        .await
        .map_err(|e| e.to_string())?;
    let existing = details.items.iter().find(|item| item.item_id == item_id);

    let new_quantity = if let Some(item) = existing {
        item.quantity + quantity
    } else {
        quantity
    };

    if new_quantity <= 0 {
        // Remove item if quantity is zero or less
        remove_item_from_transaction(transaction_id, item_id).await
    } else if new_quantity == 1 {
        // add item with quantity 1
        Request::post(&format!(
            "{}/transactions/{}/items",
            API_BASE, transaction_id
        ))
        .json(&AddTransactionItemDto {
            item_id,
            quantity: new_quantity,
        })
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
        Ok(())
    } else if new_quantity > 1 {
        // Update item quantity
        Request::put(&format!(
            "{}/transactions/{}/items/{}",
            API_BASE, transaction_id, item_id
        ))
        .json(&UpdateTransactionItemDto {
            item_id,
            quantity: new_quantity,
        })
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
        Ok(())
    } else {
        Err("Invalid quantity".to_string())
    }
}

async fn remove_item_from_transaction(transaction_id: Uuid, item_id: Uuid) -> Result<(), String> {
    // Fetch current transaction details
    let details = fetch_transaction_details(transaction_id)
        .await
        .map_err(|e| e.to_string())?;
    if let Some(item) = details.items.iter().find(|item| item.item_id == item_id) {
        if item.quantity > 1 {
            // Decrease quantity by 1
            Request::put(&format!(
                "{}/transactions/{}/items/{}",
                API_BASE, transaction_id, item_id
            ))
            .json(&UpdateTransactionItemDto {
                item_id,
                quantity: item.quantity - 1,
            })
            .map_err(|e| e.to_string())?
            .send()
            .await
            .map_err(|e| e.to_string())?;
            Ok(())
        } else if item.quantity == 1 {
            // Remove item if quantity is 1
            Request::delete(&format!(
                "{}/transactions/{}/items/{}",
                API_BASE, transaction_id, item_id
            ))
            .send()
            .await
            .map_err(|e| e.to_string())?;
            Ok(())
        } else {
            Ok(())
        }
    } else {
        Ok(())
    }
}

async fn close_transaction(id: Uuid, paid_amount: f64) -> Result<CloseTransactionResponse, String> {
    Request::post(&format!("{}/transactions/{}/close", API_BASE, id))
        .json(&CloseTransactionDto { paid_amount })
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

async fn cancel_transaction(id: Uuid) -> Result<Transaction, String> {
    Request::post(&format!("{}/transactions/{}/cancel", API_BASE, id))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

// API Client - Reports
async fn fetch_sales_report(
    start_date: DateTime<Utc>,
    end_date: DateTime<Utc>,
) -> Result<SalesReport, String> {
    Request::post(&format!("{}/reports/sales", API_BASE))
        .json(&ReportDateRange {
            start_date,
            end_date,
        })
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

async fn fetch_daily_report() -> Result<SalesReport, String> {
    Request::get(&format!("{}/reports/daily", API_BASE))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

async fn fetch_monthly_report() -> Result<SalesReport, String> {
    Request::get(&format!("{}/reports/monthly", API_BASE))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NetworkInterface {
    pub name: String,
    pub ip: String,
    pub mac: String,
    pub is_up: bool,
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

#[derive(Deserialize, Debug, Clone)]
pub struct DiscoveredDevice {
    pub ip: String,
    pub mac: String,
    pub hostname: Option<String>,
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

#[derive(Debug, Clone)]
struct Machine {
    name: String,
    mac: String,
    ip: String,
    description: Option<String>,
    turn_off_port: Option<u16>,
    can_be_turned_off: bool,
    port_forwards: Vec<PortForward>,
}

impl validator::Validate for Machine {
    fn validate(&self) -> Result<(), validator::ValidationErrors> {
        let mut errors = validator::ValidationErrors::new();

        // Add custom validation logic here if needed
        // For now, we'll just return Ok
        if self.name.is_empty() {
            errors.add("name", validator::ValidationError::new("Name is required"));
        }
        let ip = self.ip.parse::<std::net::IpAddr>();

        if ip.is_err() {
            errors.add("ip", validator::ValidationError::new("Invalid IP address"));
        }

        if self.mac.is_empty() {
            errors.add(
                "mac",
                validator::ValidationError::new("MAC address is required"),
            );
        }
        let is_valid_mac = self
            .mac
            .chars()
            .filter(|c| c.is_ascii_hexdigit() || *c == ':' || *c == '-')
            .count()
            == self.mac.len()
            && (self.mac.len() == 17 || self.mac.len() == 12);

        if !is_valid_mac {
            errors.add(
                "mac",
                validator::ValidationError::new("Invalid MAC address"),
            );
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

#[derive(Debug, Clone)]
struct PortForward {
    name: Option<String>,
    local_port: u16,
    target_port: u16,
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
                    console::log_1(&format!("Error scanning network: {}", err).into());
                });

            set_loading.set(false);
        });
    };

    fn handle_add_machine(device: DiscoveredDevice, set_machine: WriteSignal<Machine>) {
        let new_machine = Machine {
            name: device
                .hostname
                .clone()
                .unwrap_or_else(|| "Unnamed Device".to_string()),
            mac: device.mac.clone(),
            ip: device.ip.clone(),
            description: None,
            turn_off_port: None,
            can_be_turned_off: false,
            port_forwards: vec![],
        };
        set_machine.set(new_machine);
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
                                                    handle_add_machine(device.clone(), set_machine.clone());
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
fn RegistredMachines() -> impl IntoView {
    let machines = vec![Machine {
        name: "Work Laptop".to_string(),
        mac: "AA:BB:CC:DD:EE:FF".to_string(),
        ip: "192.168.0.1".to_string(),
        description: Some("My work laptop".to_string()),
        turn_off_port: Some(9),
        can_be_turned_off: true,
        port_forwards: vec![
            PortForward {
                name: Some("SSH".to_string()),
                local_port: 22,
                target_port: 2222,
            },
            PortForward {
                name: Some("Web".to_string()),
                local_port: 80,
                target_port: 8080,
            },
        ],
    }];

    view! {
        <section class="mt-8">
            <div class="mb-3 flex items-center justify-between">
                <h2 class="text-lg font-semibold">Registered Machines</h2>
            </div>
            <div class="overflow-x-auto rounded-2xl border border-gray-200 bg-white shadow-sm dark:border-gray-800 dark:bg-gray-900">
                <table class="min-w-full text-left text-sm">
                    <thead class="bg-gray-50 text-gray-600 dark:bg-gray-950 dark:text-gray-300">
                        <tr>
                            <th class="px-4 py-3 font-semibold">Name</th>
                            <th class="px-4 py-3 font-semibold">MAC Address</th>
                            <th class="px-4 py-3 font-semibold">IP Address</th>
                            <th class="px-4 py-3 font-semibold">Description</th>
                            <th class="px-4 py-3 font-semibold">Turn Off Port</th>
                            <th class="px-4 py-3 font-semibold">Can Be Turned Off</th>
                            <th class="px-4 py-3 font-semibold">Status</th>
                            <th class="px-4 py-3 font-semibold w-64">Port Forwards</th>
                            <th class="px-4 py-3 font-semibold">Action</th>
                        </tr>
                    </thead>
                    <tbody class="divide-y divide-gray-100 dark:divide-gray-800">
                        {move || {
                            machines
                                .iter()
                                .map(|m| {
                                    view! {
                                        <tr class="align-middle">
                                            <td class="px-4 py-3 text-xs sm:text-sm">
                                                <a
                                                    class="underline text-blue-700 dark:text-blue-400 hover:text-blue-900"
                                                    href="/machines/{ m.mac }"
                                                >
                                                    {m.name.clone()}
                                                </a>
                                            </td>
                                            <td class="px-4 py-3 font-mono text-xs sm:text-sm">
                                                {m.mac.clone()}
                                            </td>
                                            <td class="px-4 py-3 font-mono text-xs sm:text-sm">
                                                {m.ip.clone()}
                                            </td>
                                            <td class="px-4 py-3">
                                                <span class="text-xs sm:text-sm">"bla"</span>
                                            </td>
                                            <td class="px-4 py-3">
                                                <span class="font-mono text-xs sm:text-sm">9090</span>
                                            </td>
                                            <td class="px-4 py-3">
                                                <span class="text-xs sm:text-sm">
                                                    {m.can_be_turned_off}
                                                </span>
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
fn AddMachine(machine: ReadSignal<Machine>, set_machine: WriteSignal<Machine>) -> impl IntoView {
    let (machine_form_data, set_machine_form_data) = create_signal::<Machine>(machine.get());
    let (erros, set_errors) = create_signal::<HashMap<String, Vec<String>>>(HashMap::new());

    // Update the local signal when the incoming signal changes
    Effect::new(move |_| {
        set_machine_form_data.set(machine.get());
    });

    fn set_input_value(
        key: &str,
        value: String,
        set_machine_form_data: WriteSignal<Machine>,
        machine_form_data: ReadSignal<Machine>,
    ) {
        console_log(&format!("Setting {} to {}", key, value));
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
        match machine_form_data.get().validate() {
            Ok(_) => {
                console::log_1(&"Form is valid".into());
                // Update the parent machine signal with the form data
                set_machine.set(machine_form_data.get());
            }
            Err(e) => {
                let mut new_errors = HashMap::new();
                console::log_1(&format!("Form is invalid: {:?}", e).into());
                for (field, errors) in e.field_errors() {
                    let mut field_errors = vec![];
                    console::log_1(&format!("Field: {}", field).into());
                    for error in errors {
                        console_log(&format!(" - Code: {}", error.code));
                        field_errors.push(error.code.to_string());
                        console::log_1(
                            &format!(" - Error: {}", error.message.clone().unwrap_or_default())
                                .into(),
                        );
                    }
                    new_errors.insert(field.to_string(), field_errors);
                }
                set_errors.set(new_errors);
                return;
            }
        }

        console::log_1(&format!("Form submitted with data: {:?}", ev).into());
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
                            prop:value=machine_form_data.get().name
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

                            prop:value=machine_form_data
                                .get()
                                .description
                                .clone()
                                .unwrap_or_default()
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
                        prop:checked=machine_form_data.get().can_be_turned_off
                        id="can_be_turned_off"
                        name="can_be_turned_off"
                        class=""
                    />
                </div>
                <div style="font-size:21px; display: flex;justify-content: center;">

                    <button type="submit" class="submit-button submit-button:hover">
                        "Add Machine"
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
    view! {
        <Header set_machine=set_machine />
        <AddMachine machine=machine set_machine=set_machine />
    }
}

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(App)
}
