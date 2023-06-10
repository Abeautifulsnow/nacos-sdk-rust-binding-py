#![deny(clippy::all)]

use pyo3::exceptions::PyRuntimeError;
use pyo3::types::PyFunction;
use pyo3::{pyclass, pymethods, PyResult};

use std::sync::Arc;

/// Client api of Nacos Config.
#[pyclass]
pub struct NacosConfigClient {
    inner: Arc<dyn nacos_sdk::api::config::ConfigService + Send + Sync + 'static>,
}

#[pymethods]
impl NacosConfigClient {
    /// Build a Config Client.
    #[new]
    pub fn new(client_options: crate::ClientOptions) -> PyResult<NacosConfigClient> {
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

        let config_service_builder = if is_enable_auth {
            nacos_sdk::api::config::ConfigServiceBuilder::new(props).enable_auth_plugin_http()
        } else {
            nacos_sdk::api::config::ConfigServiceBuilder::new(props)
        };

        let config_service = config_service_builder
            .build()
            .map_err(|nacos_err| PyRuntimeError::new_err(format!("{:?}", &nacos_err)))?;

        Ok(NacosConfigClient {
            inner: Arc::new(config_service),
        })
    }

    /// Get config's content.
    /// If it fails, pay attention to err
    pub async fn get_config(&self, data_id: String, group: String) -> PyResult<String> {
        let resp = self.get_config_resp(data_id, group).await?;
        Ok(resp.content)
    }

    /// Get NacosConfigResponse.
    /// If it fails, pay attention to err
    pub async fn get_config_resp(
        &self,
        data_id: String,
        group: String,
    ) -> PyResult<NacosConfigResponse> {
        let config_resp = self
            .inner
            .get_config(data_id, group)
            .await
            .map_err(|nacos_err| PyRuntimeError::new_err(format!("{:?}", &nacos_err)))?;
        Ok(transfer_conf_resp(config_resp))
    }

    /// Publish config.
    /// If it fails, pay attention to err
    pub async fn publish_config(
        &self,
        data_id: String,
        group: String,
        content: String,
    ) -> PyResult<bool> {
        self.inner
            .publish_config(data_id, group, content, None)
            .await
            .map_err(|nacos_err| PyRuntimeError::new_err(format!("{:?}", &nacos_err)))
    }

    /// Remove config.
    /// If it fails, pay attention to err
    pub async fn remove_config(&self, data_id: String, group: String) -> PyResult<bool> {
        self.inner
            .remove_config(data_id, group)
            .await
            .map_err(|nacos_err| PyRuntimeError::new_err(format!("{:?}", &nacos_err)))
    }

    /// Add NacosConfigChangeListener callback func, which listen the config change.
    /// If it fails, pay attention to err
    pub async fn add_listener(
        &self,
        data_id: String,
        group: String,
        listener: PyFunction, // arg: <NacosConfigResponse>
    ) -> PyResult<()> {
        self.inner
            .add_listener(
                data_id,
                group,
                Arc::new(NacosConfigChangeListener {
                    func: Arc::new(listener),
                }),
            )
            .await
            .map_err(|nacos_err| PyRuntimeError::new_err(format!("{:?}", &nacos_err)))?;
        Ok(())
    }

    /// Remove NacosConfigChangeListener callback func, but noop....
    /// The logic is not implemented internally, and only APIs are provided as compatibility.
    /// Users maybe do not need it? Not removing the listener is not a big problem, Sorry!

    pub async fn remove_listener(
        &self,
        _data_id: String,
        _group: String,
        _listener: PyFunction, // arg: <NacosConfigResponse>
    ) -> PyResult<()> {
        Ok(())
    }
}

#[pyclass]
pub struct NacosConfigResponse {
    /// Namespace/Tenant
    pub namespace: String,
    /// DataId
    pub data_id: String,
    /// Group
    pub group: String,
    /// Content
    pub content: String,
    /// Content's Type; e.g. json,properties,xml,html,text,yaml
    pub content_type: String,
    /// Content's md5
    pub md5: String,
}

pub struct NacosConfigChangeListener {
    func: Arc<PyFunction>,
}

impl nacos_sdk::api::config::ConfigChangeListener for NacosConfigChangeListener {
    fn notify(&self, config_resp: nacos_sdk::api::config::ConfigResponse) {
        let listen = self.func.clone();

        let ffi_conf_resp = transfer_conf_resp(config_resp);

        // todo call PyFunction with args
        std::thread::spawn(move || {
            let _ = listen.call(Ok(ffi_conf_resp), None);
        });
    }
}

fn transfer_conf_resp(config_resp: nacos_sdk::api::config::ConfigResponse) -> NacosConfigResponse {
    NacosConfigResponse {
        namespace: config_resp.namespace().to_string(),
        data_id: config_resp.data_id().to_string(),
        group: config_resp.group().to_string(),
        content: config_resp.content().to_string(),
        content_type: config_resp.content_type().to_string(),
        md5: config_resp.md5().to_string(),
    }
}