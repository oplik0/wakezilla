use chrono::{DateTime, Utc};
use gloo_net::http::Request;
use leptos::{leptos_dom::logging::console_log, prelude::*};
use leptos_meta::*;
use leptos_router::{
    StaticSegment,
    components::{A, Route, Router, Routes},
};

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

#[derive(Debug, Clone)]
struct PortForward {
    name: Option<String>,
    local_port: u16,
    target_port: u16,
}

#[component]
fn Navbar() -> impl IntoView {
    view! {
        <nav class="navbar">
            <div class="nav-container">
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
// Components
#[component]
fn App() -> impl IntoView {
    provide_meta_context();

    view! {
        <Html attr:lang="en" />
        <Stylesheet id="leptos" href="/style/main.css" />
        <Title text="Wakezilla" />
        <Stylesheet href="https://cdn.tailwindcss.com" />
        <head>
            <meta charset="UTF-8" />
            <meta
                name="viewport"
                content="width=device-width, initial-scale=1, viewport-fit=cover"
            />
            <title>Wakezilla</title>
            <script src="https://cdn.tailwindcss.com"></script>
        </head>
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
fn Header() -> impl IntoView {
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

    Effect::new(move |loading| {
        console_log(&format!("Loading state changed"));
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

    view! {
        <header class="mb-6 flex flex-col items-start justify-between gap-4 sm:flex-row sm:items-center">
            <div>
                <h1 class="text-2xl font-bold tracking-tight">Wakezilla Manager</h1>
                <p class="mt-1 text-sm text-gray-600 dark:text-gray-400">
                    Wake, manage, and forward to your registered machines.
                </p>
            </div>
            <form on:submit=on_submit class="flex items-center gap-2">
                <select
                    id="interface-select"
                    class="rounded-xl border border-gray-300 bg-white px-3 py-2 text-sm font-medium shadow-sm transition hover:bg-gray-50 dark:border-gray-700 dark:bg-gray-900 dark:hover:bg-gray-800"
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
                <button
                    id="scan-btn"
                    class="inline-flex items-center justify-center rounded-xl border border-gray-300 bg-white px-4 py-2 text-sm font-medium shadow-sm transition hover:bg-gray-50 active:scale-[0.99] disabled:cursor-not-allowed disabled:opacity-60 dark:border-gray-700 dark:bg-gray-900 dark:hover:bg-gray-800"
                >
                    {"🔍"}
                    Scan Network
                </button>
            </form>

        </header>
        <p>{move || if loading.get() { "Scanning..." } else { "" }}</p>
        <Show when=move || { !discovered_devices.get().is_empty() } fallback=|| view! { "" }>

            <section id="scan-results-container">
                <div class="mb-3 flex items-center justify-between">
                    <h2 class="text-lg font-semibold">Discovered Devices</h2>
                    <span
                        id="scan-status"
                        class="text-sm text-gray-500 dark:text-gray-400"
                        aria-live="polite"
                    ></span>
                </div>
                <div class="overflow-x-auto rounded-2xl border border-gray-200 bg-white shadow-sm dark:border-gray-800 dark:bg-gray-900">
                    <table id="scan-results-table" class="min-w-full text-left text-sm">
                        <thead class="bg-gray-50 text-gray-600 dark:bg-gray-950 dark:text-gray-300">
                            <tr>
                                <th class="px-4 py-3 font-semibold">IP Address</th>
                                <th class="px-4 py-3 font-semibold">Hostname</th>
                                <th class="px-4 py-3 font-semibold">MAC Address</th>
                                <th class="px-4 py-3 font-semibold">Action</th>
                            </tr>
                        </thead>
                        <tbody class="divide-y divide-gray-100 dark:divide-gray-800">
                            <For
                                each=move || discovered_devices.get()
                                key=|device| device.ip.clone()
                                children=move |device| {
                                    view! {
                                        <tr>
                                            <td class="px-4 py-3 font-mono text-xs sm:text-sm">
                                                {device.ip.clone()}
                                            </td>
                                            <td class="px-4 py-3 text-xs sm:text-sm">
                                                {device
                                                    .hostname
                                                    .clone()
                                                    .unwrap_or_else(|| "N/A".to_string())}
                                            </td>
                                            <td class="px-4 py-3 font-mono text-xs sm:text-sm">
                                                {device.mac.clone()}
                                            </td>
                                            <td class="px-4 py-3">
                                                <button class="text-blue-600 hover:underline text-sm">
                                                    "Wake"
                                                </button>
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
fn HomePage() -> impl IntoView {
    view! { <Header /> }
}

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(App)
}
