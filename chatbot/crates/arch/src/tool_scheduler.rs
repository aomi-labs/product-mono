use crate::{UnifiedApiClient, ApiRequest, ApiResponse};
use futures::stream::{FuturesUnordered, StreamExt};
use std::sync::{Arc, OnceLock};
use tokio::{
    runtime::{Runtime, Handle},
    sync::{mpsc, oneshot},
};

static TOKIO_SINGLETON: OnceLock<std::io::Result<Arc<tokio::runtime::Runtime>>> = OnceLock::new();


/// Scheduler manages concurrent tool execution requests and contains the scheduling loop
pub struct Scheduler {
    requests_rx: mpsc::Receiver<(ApiRequest, oneshot::Sender<Result<ApiResponse, String>>)>,
    responses_tx: mpsc::Sender<Result<ApiResponse, String>>,
    client: Arc<UnifiedApiClient>,
    runtime: Arc<tokio::runtime::Runtime>
}

/// Client interface for submitting requests to the scheduler
pub struct SchedulerClient {
    requests_tx: mpsc::Sender<(ApiRequest, oneshot::Sender<Result<ApiResponse, String>>)>,
}

/// Handler receives responses from completed tool executions
pub struct ApiHandler {
    responses_rx: mpsc::Receiver<Result<ApiResponse, String>>,
}

impl ApiHandler {
    fn new(responses_rx: mpsc::Receiver<Result<ApiResponse, String>>) -> Self {
        Self { responses_rx }
    }

    /// Get the next completed response (non-blocking)
    pub async fn next_response(&mut self) -> Option<Result<ApiResponse, String>> {
        self.responses_rx.recv().await
    }
}

impl Scheduler {
    /// Create a new scheduler (does not start the loop)
    pub fn new(client: Arc<UnifiedApiClient>) -> (SchedulerClient, ApiHandler, Self) {
        let (req_tx, req_rx) =
            mpsc::channel::<(ApiRequest, oneshot::Sender<Result<ApiResponse, String>>)>(100);
        let (resp_tx, resp_rx) = mpsc::channel::<Result<ApiResponse, String>>(100);

        let client_interface = SchedulerClient {
            requests_tx: req_tx,
        };

        let handler = ApiHandler::new(resp_rx);
        
        let scheduler = Scheduler {
            requests_rx: req_rx,
            responses_tx: resp_tx,
            client,
            runtime: Self::get_tokio_singleton().unwrap()
        };

        (client_interface, handler, scheduler)
    }

    /// Convenience method to create scheduler with default unified client
    pub fn new_with_default_client() -> (SchedulerClient, ApiHandler, Self) {
        let client = Arc::new(UnifiedApiClient::new());
        Self::new(client)
    }

    fn get_tokio_singleton() -> Result<Arc<tokio::runtime::Runtime>, String> {
        match TOKIO_SINGLETON.get_or_init(|| tokio::runtime::Runtime::new().map(Arc::new)) {
            Ok(t) => Ok(t.clone()),
            Err(e) => Err("Failed to get singleton runtime".to_string()),
        }
    }

    pub fn run(self) {
        let Scheduler { requests_rx, responses_tx, client, runtime} = self;
        
        runtime.spawn(async move {
            let mut jobs = FuturesUnordered::new();
            let mut requests_rx = requests_rx;

            loop {
                tokio::select! {
                    // Accept new request if available
                    maybe_req = requests_rx.recv() => {
                        if let Some((req, reply_tx)) = maybe_req {
                            let client = client.clone();
                            let resp_tx = responses_tx.clone();

                            // Each request becomes an async job
                            jobs.push(async move {
                                let result = match client.call_api(req.clone()).await {
                                    Ok(response) => Ok(response),
                                    Err(e) => Err(e),
                                };

                                // Notify handler via resp_tx
                                let _ = resp_tx.send(result.clone()).await;

                                // Respond to any awaiting oneshot listener
                                let _ = reply_tx.send(result);
                            });
                        } else {
                            break; // all senders dropped
                        }
                    }

                    // Process completed requests
                    Some(_) = jobs.next() => {
                        // FuturesUnordered automatically drives concurrency
                    }

                    else => break,
                }
            }
        });
    }
}

impl SchedulerClient {
    /// Schedule an API request; returns a oneshot receiver for the result.
    pub async fn schedule(&self, req: ApiRequest) -> oneshot::Receiver<Result<ApiResponse, String>> {
        let (tx, rx) = oneshot::channel();
        let _ = self.requests_tx.send((req, tx)).await;
        rx
    }

    /// Schedule an API request synchronously (non-blocking)
    pub fn schedule_sync(&self, req: ApiRequest) -> oneshot::Receiver<Result<ApiResponse, String>> {
        let (tx, rx) = oneshot::channel();
        // Use try_send for non-blocking operation
        match self.requests_tx.try_send((req, tx)) {
            Ok(_) => rx,
            Err(_) => {
                // If channel is full, create a failed receiver
                let (failed_tx, failed_rx) = oneshot::channel();
                let _ = failed_tx.send(Err("Scheduler queue full".to_string()));
                failed_rx
            }
        }
    }

    /// Convenience method to schedule a contract request
    pub async fn schedule_contract(&self, contract_id: String, query: String) -> oneshot::Receiver<Result<ApiResponse, String>> {
        let request = UnifiedApiClient::contract_request(contract_id, query);
        self.schedule(request).await
    }

    /// Convenience method to schedule a weather request
    pub async fn schedule_weather(&self, weather_id: String, query: String) -> oneshot::Receiver<Result<ApiResponse, String>> {
        let request = UnifiedApiClient::weather_request(weather_id, query);
        self.schedule(request).await
    }

    /// Convenience method to schedule a contract request synchronously
    pub fn schedule_contract_sync(&self, contract_id: String, query: String) -> oneshot::Receiver<Result<ApiResponse, String>> {
        let request = UnifiedApiClient::contract_request(contract_id, query);
        self.schedule_sync(request)
    }

    /// Convenience method to schedule a weather request synchronously
    pub fn schedule_weather_sync(&self, weather_id: String, query: String) -> oneshot::Receiver<Result<ApiResponse, String>> {
        let request = UnifiedApiClient::weather_request(weather_id, query);
        self.schedule_sync(request)
    }
}

#[tokio::test]
async fn test_scheduler_contract() {
    let (scheduler_client, mut handler, scheduler) = Scheduler::new_with_default_client();

    // Start the scheduler
    scheduler.run();

    // Schedule a contract request
    let receiver = scheduler_client.schedule_contract("0x123".to_string(), "balance".to_string()).await;
    
    // Wait for response
    let result = receiver.await.unwrap();
    assert!(result.is_ok());
    assert!(result.as_ref().unwrap().is_contract());

    // Check handler also received it
    let handler_response = handler.next_response().await.unwrap();
    assert!(handler_response.is_ok());
    assert!(handler_response.as_ref().unwrap().is_contract());
}

#[tokio::test]
async fn test_scheduler_weather() {
    let (scheduler_client, mut handler, scheduler) = Scheduler::new_with_default_client();

    // Start the scheduler
    scheduler.run();

    // Schedule a weather request
    let receiver = scheduler_client.schedule_weather("weather123".to_string(), "temperature".to_string()).await;
    
    // Wait for response
    let result = receiver.await.unwrap();
    assert!(result.is_ok());
    assert!(result.as_ref().unwrap().is_weather());

    // Check handler also received it
    let handler_response = handler.next_response().await.unwrap();
    assert!(handler_response.is_ok());
    assert!(handler_response.as_ref().unwrap().is_weather());
}

#[tokio::test]
async fn test_scheduler_mixed_requests() {
    let (scheduler_client, mut handler, scheduler) = Scheduler::new_with_default_client();

    // Start the scheduler
    scheduler.run();

    // Schedule both contract and weather requests
    let contract_receiver = scheduler_client.schedule_contract("0x456".to_string(), "owner".to_string()).await;
    let weather_receiver = scheduler_client.schedule_weather("weather456".to_string(), "humidity".to_string()).await;
    
    // Wait for responses
    let contract_result = contract_receiver.await.unwrap();
    let weather_result = weather_receiver.await.unwrap();
    
    assert!(contract_result.is_ok());
    assert!(contract_result.as_ref().unwrap().is_contract());
    
    assert!(weather_result.is_ok());
    assert!(weather_result.as_ref().unwrap().is_weather());

    // Check handler received both
    let response1 = handler.next_response().await.unwrap();
    let response2 = handler.next_response().await.unwrap();
    
    assert!(response1.is_ok());
    assert!(response2.is_ok());
    
    // One should be contract, one should be weather
    let responses = vec![response1.unwrap(), response2.unwrap()];
    assert!(responses.iter().any(|r| r.is_contract()));
    assert!(responses.iter().any(|r| r.is_weather()));
}
