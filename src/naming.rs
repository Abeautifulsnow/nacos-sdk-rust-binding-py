#![deny(clippy::all)]

use pyo3::exceptions::PyRuntimeError;
use pyo3::types::PyFunction;
use pyo3::{pyclass, pymethods, PyResult};

use std::sync::Arc;

/// Client api of Nacos Naming.
#[pyclass]
pub struct NacosNamingClient {
    inner: Arc<dyn nacos_sdk::api::naming::NamingService + Send + Sync + 'static>,
}

#[pymethods]
impl NacosNamingClient {
    /// Build a Naming Client.
    #[new]
    pub fn new(client_options: crate::ClientOptions) -> PyResult<NacosNamingClient> {
        // print to console or file
        let _ = crate::init_logger();

        let props = nacos_sdk::api::props::ClientProps::new()
            .server_addr(client_options.server_addr)
            .namespace(client_options.namespace)
            .app_name(
                client_options
                    .app_name
                    .unwrap_or(nacos_sdk::api::constants::UNKNOWN.to_string()),
            );

        // need enable_auth_plugin_http with username & password
        let is_enable_auth = client_options.username.is_some() && client_options.password.is_some();

        let props = if is_enable_auth {
            props
                .auth_username(client_options.username.unwrap())
                .auth_password(client_options.password.unwrap())
        } else {
            props
        };

        let naming_service_builder = if is_enable_auth {
            nacos_sdk::api::naming::NamingServiceBuilder::new(props).enable_auth_plugin_http()
        } else {
            nacos_sdk::api::naming::NamingServiceBuilder::new(props)
        };

        let naming_service = naming_service_builder
            .build()
            .map_err(|nacos_err| PyRuntimeError::new_err(format!("{:?}", &nacos_err)))?;

        Ok(NacosNamingClient {
            inner: Arc::new(naming_service),
        })
    }

    /// Register instance.
    /// If it fails, pay attention to err
    pub async fn register_instance(
        &self,
        service_name: String,
        group: String,
        service_instance: NacosServiceInstance,
    ) -> PyResult<()> {
        self.inner
            .register_instance(
                service_name,
                Some(group),
                transfer_ffi_instance_to_rust(&service_instance),
            )
            .await
            .map_err(|nacos_err| PyRuntimeError::new_err(format!("{:?}", &nacos_err)))
    }

    /// Deregister instance.
    /// If it fails, pay attention to err
    pub async fn deregister_instance(
        &self,
        service_name: String,
        group: String,
        service_instance: NacosServiceInstance,
    ) -> PyResult<()> {
        self.inner
            .deregister_instance(
                service_name,
                Some(group),
                transfer_ffi_instance_to_rust(&service_instance),
            )
            .await
            .map_err(|nacos_err| PyRuntimeError::new_err(format!("{:?}", &nacos_err)))
    }

    /// Batch register instance, improve interaction efficiency.
    /// If it fails, pay attention to err
    pub async fn batch_register_instance(
        &self,
        service_name: String,
        group: String,
        service_instances: Vec<NacosServiceInstance>,
    ) -> PyResult<()> {
        let rust_instances = service_instances
            .iter()
            .map(transfer_ffi_instance_to_rust)
            .collect();

        self.inner
            .batch_register_instance(service_name, Some(group), rust_instances)
            .await
            .map_err(|nacos_err| PyRuntimeError::new_err(format!("{:?}", &nacos_err)))
    }

    /// Get all instances by service and group. default cluster=[], subscribe=true.
    /// If it fails, pay attention to err
    pub async fn get_all_instances(
        &self,
        service_name: String,
        group: String,
        clusters: Option<Vec<String>>,
        subscribe: Option<bool>,
    ) -> PyResult<Vec<NacosServiceInstance>> {
        let rust_instances = self
            .inner
            .get_all_instances(
                service_name,
                Some(group),
                clusters.unwrap_or_default(),
                subscribe.unwrap_or(true),
            )
            .await
            .map_err(|nacos_err| PyRuntimeError::new_err(format!("{:?}", &nacos_err)))?;

        Ok(rust_instances
            .iter()
            .map(transfer_rust_instance_to_ffi)
            .collect())
    }

    /// Select instances whether healthy or not. default cluster=[], subscribe=true, healthy=true.
    /// If it fails, pay attention to err
    pub async fn select_instances(
        &self,
        service_name: String,
        group: String,
        clusters: Option<Vec<String>>,
        subscribe: Option<bool>,
        healthy: Option<bool>,
    ) -> PyResult<Vec<NacosServiceInstance>> {
        let rust_instances = self
            .inner
            .select_instances(
                service_name,
                Some(group),
                clusters.unwrap_or_default(),
                subscribe.unwrap_or(true),
                healthy.unwrap_or(true),
            )
            .await
            .map_err(|nacos_err| PyRuntimeError::new_err(format!("{:?}", &nacos_err)))?;

        Ok(rust_instances
            .iter()
            .map(transfer_rust_instance_to_ffi)
            .collect())
    }

    /// Select one healthy instance. default cluster=[], subscribe=true.
    /// If it fails, pay attention to err
    pub async fn select_one_healthy_instance(
        &self,
        service_name: String,
        group: String,
        clusters: Option<Vec<String>>,
        subscribe: Option<bool>,
    ) -> PyResult<NacosServiceInstance> {
        let rust_instance = self
            .inner
            .select_one_healthy_instance(
                service_name,
                Some(group),
                clusters.unwrap_or_default(),
                subscribe.unwrap_or(true),
            )
            .await
            .map_err(|nacos_err| PyRuntimeError::new_err(format!("{:?}", &nacos_err)))?;

        Ok(transfer_rust_instance_to_ffi(&rust_instance))
    }

    /// Add NacosNamingEventListener callback func, which listen the instance change.
    /// If it fails, pay attention to err
    pub async fn subscribe(
        &self,
        service_name: String,
        group: String,
        clusters: Option<Vec<String>>,
        listener: PyFunction, // arg: Vec<NacosServiceInstance>
    ) -> PyResult<()> {
        self.inner
            .subscribe(
                service_name,
                Some(group),
                clusters.unwrap_or_default(),
                Arc::new(NacosNamingEventListener {
                    func: Arc::new(listener),
                }),
            )
            .await
            .map_err(|nacos_err| PyRuntimeError::new_err(format!("{:?}", &nacos_err)))?;
        Ok(())
    }

    /// Remove NacosNamingEventListener callback func, but noop....
    /// The logic is not implemented internally, and only APIs are provided as compatibility.
    /// Users maybe do not need it? Not removing the subscription is not a big problem, Sorry!
    pub async fn un_subscribe(
        &self,
        _service_name: String,
        _group: String,
        _clusters: Option<Vec<String>>,
        _listener: PyFunction, // arg: Vec<NacosServiceInstance>
    ) -> PyResult<()> {
        Ok(())
    }
}

pub struct NacosNamingEventListener {
    func: Arc<PyFunction>,
}

impl nacos_sdk::api::naming::NamingEventListener for NacosNamingEventListener {
    fn event(&self, event: Arc<nacos_sdk::api::naming::NamingChangeEvent>) {
        let listen = self.func.clone();

        if event.instances.is_none() {
            return;
        }

        let rust_instances = event.instances.clone().unwrap();

        let ffi_instances = rust_instances
            .iter()
            .map(transfer_rust_instance_to_ffi)
            .collect();

        // todo call PyFunction with args
        std::thread::spawn(move || {
            let _ = listen.call(Ok(ffi_instances), None);
        });
    }
}

#[pyclass]
pub struct NacosServiceInstance {
    /// Instance Id
    pub instance_id: Option<String>,
    /// Ip
    pub ip: String,
    /// Port
    pub port: i32,
    /// Weight, default 1.0
    pub weight: Option<f64>,
    /// Healthy or not, default true
    pub healthy: Option<bool>,
    /// Enabled ot not, default true
    pub enabled: Option<bool>,
    /// Ephemeral or not, default true
    pub ephemeral: Option<bool>,
    /// Cluster Name, default 'DEFAULT'
    pub cluster_name: Option<String>,
    /// Service Name
    pub service_name: Option<String>,
    /// Metadata, default '{}'
    pub metadata: Option<std::collections::HashMap<String, String>>,
}

fn transfer_ffi_instance_to_rust(
    ffi_instance: &NacosServiceInstance,
) -> nacos_sdk::api::naming::ServiceInstance {
    nacos_sdk::api::naming::ServiceInstance {
        instance_id: ffi_instance.instance_id.clone(),
        ip: ffi_instance.ip.clone(),
        port: ffi_instance.port,
        weight: ffi_instance.weight.unwrap_or(1.0),
        healthy: ffi_instance.healthy.unwrap_or(true),
        enabled: ffi_instance.enabled.unwrap_or(true),
        ephemeral: ffi_instance.ephemeral.unwrap_or(true),
        cluster_name: ffi_instance.cluster_name.clone(),
        service_name: ffi_instance.service_name.clone(),
        metadata: ffi_instance.metadata.clone().unwrap_or_default(),
    }
}

fn transfer_rust_instance_to_ffi(
    rust_instance: &nacos_sdk::api::naming::ServiceInstance,
) -> NacosServiceInstance {
    NacosServiceInstance {
        instance_id: rust_instance.instance_id.clone(),
        ip: rust_instance.ip.clone(),
        port: rust_instance.port,
        weight: Some(rust_instance.weight),
        healthy: Some(rust_instance.healthy),
        enabled: Some(rust_instance.enabled),
        ephemeral: Some(rust_instance.ephemeral),
        cluster_name: rust_instance.cluster_name.clone(),
        service_name: rust_instance.service_name.clone(),
        metadata: Some(rust_instance.metadata.clone()),
    }
}