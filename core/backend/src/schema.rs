use prost::Message;
use prost_reflect::{DescriptorPool, DynamicMessage, MethodDescriptor, SerializeOptions};
use serde_json::Value;
use std::fs;
use std::path::Path;
use prost_reflect::Kind;

/// Header of xresloader binary tables (*.bytes)
#[derive(Clone, PartialEq, Message)]
pub struct XresHeader {
    #[prost(string, tag = "1")]
    pub xres_ver: String,
    #[prost(string, tag = "2")]
    pub data_ver: String,
    #[prost(uint32, tag = "3")]
    pub count: u32,
    #[prost(string, tag = "4")]
    pub hash_code: String,
}

/// Container of ExcelConfig/*.bytes binary tables
#[derive(Clone, PartialEq, Message)]
pub struct XresDataBlocks {
    #[prost(message, optional, tag = "1")]
    pub header: Option<XresHeader>,
    #[prost(bytes = "vec", repeated, tag = "2")]
    pub data_block: Vec<Vec<u8>>,
    #[prost(string, tag = "3")]
    pub data_message_type: String,
}

/// Handles dynamic Protobuf <-> JSON translation using the game's schemas.
pub struct SchemaRegistry {
    pub pool: DescriptorPool,
    pub ser_options: SerializeOptions,
}

impl SchemaRegistry {
    /// Loads compiled FileDescriptorSets (Network.pb and Configure.pb)
    /// 
    pub fn dump_schema(&self, path: &str) -> std::io::Result<()> {
        use std::fmt::Write as _;
        let mut o = String::new();
        for svc in self.pool.services() {
            let _ = writeln!(o, "service {} {{", svc.full_name());
            for m in svc.methods() {
                let _ = writeln!(o, "  rpc {}({}) returns ({});", m.name(), m.input().full_name(), m.output().full_name());
            }
            let _ = writeln!(o, "}}\n");
        }
        for msg in self.pool.all_messages() {
            let _ = writeln!(o, "message {} {{", msg.full_name());
            for f in msg.fields() {
                let ty = match f.kind() {
                    Kind::Message(m) => m.full_name().to_string(),
                    Kind::Enum(e) => e.full_name().to_string(),
                    k => format!("{:?}", k).to_lowercase(),
                };
                let label = if f.is_map() { "map " } else if f.is_list() { "repeated " } else { "" };
                let _ = writeln!(o, "  {}{} {} = {};", label, ty, f.name(), f.number());
            }
            let _ = writeln!(o, "}}\n");
        }
        for e in self.pool.all_enums() {
            let _ = writeln!(o, "enum {} {{", e.full_name());
            for v in e.values() { let _ = writeln!(o, "  {} = {};", v.name(), v.number()); }
            let _ = writeln!(o, "}}\n");
        }
        std::fs::write(path, o)
    }
    pub fn load_from_files(
        network_pb_path: impl AsRef<Path>,
        configure_pb_path: impl AsRef<Path>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let mut pool = DescriptorPool::new();
        pool.decode_file_descriptor_set(fs::read(network_pb_path)?.as_slice())?;
        pool.decode_file_descriptor_set(fs::read(configure_pb_path)?.as_slice())?;

        // Important: use_proto_field_name(true) keeps snake_case field names,
        // as they are declared in the game's .proto files.
        let ser_options = SerializeOptions::new()
            .use_proto_field_name(true)
            .skip_default_fields(true);

        Ok(Self { pool, ser_options })
    }

    /// Decodes a binary Protobuf buffer into serde_json::Value by message name
    pub fn decode_to_json(&self, type_name: &str, bytes: &[u8]) -> Result<Value, String> {
        let clean = type_name.trim_start_matches('.');
        let desc = self
            .pool
            .get_message_by_name(clean)
            .ok_or_else(|| format!("Message '{}' not found", clean))?;
        let dyn_msg = DynamicMessage::decode(desc, bytes).map_err(|e| e.to_string())?;

        let mut buf = Vec::new();
        let mut ser = serde_json::Serializer::new(&mut buf);
        dyn_msg
            .serialize_with_options(&mut ser, &self.ser_options)
            .map_err(|e| e.to_string())?;
        serde_json::from_slice(&buf).map_err(|e| e.to_string())
    }

    /// Encodes serde_json::Value back into a binary Protobuf buffer
    pub fn encode_from_json(&self, type_name: &str, json: &Value) -> Result<Vec<u8>, String> {
        let clean = type_name.trim_start_matches('.');
        let desc = self
            .pool
            .get_message_by_name(clean)
            .ok_or_else(|| format!("Message '{}' not found", clean))?;
        let json_str = serde_json::to_string(json).map_err(|e| e.to_string())?;
        let mut de = serde_json::Deserializer::from_str(&json_str);
        let dyn_msg = DynamicMessage::deserialize(desc, &mut de)
            .map_err(|e| format!("JSON -> Proto error for {}: {}", clean, e))?;
        Ok(dyn_msg.encode_to_vec())
    }

    /// Looks up an RPC method description in a service (e.g. "tgf.GameClientService.login")
    pub fn find_rpc_method(&self, full_method: &str) -> Option<MethodDescriptor> {
        let (svc_name, method_name) = full_method.rsplit_once('.')?;
        let svc = self.pool.get_service_by_name(svc_name)?;
        let x = svc.methods().find(|m| m.name() == method_name); x
    }

    /// Automatically determines the CS...Req input type by method name and decodes it into JSON
    pub fn decode_c2s_request(&self, full_method: &str, body: &[u8]) -> Result<Value, String> {
        let method = self
            .find_rpc_method(full_method)
            .ok_or_else(|| format!("Unknown RPC method: {}", full_method))?;
        self.decode_to_json(method.input().full_name(), body)
    }

    /// Automatically determines the SC...Rsp (or Sync) output type and encodes JSON into bytes
    pub fn encode_s2c_response(
        &self,
        full_method: &str,
        json_rsp: &Value,
    ) -> Result<(String, Vec<u8>), String> {
        let method = self
            .find_rpc_method(full_method)
            .ok_or_else(|| format!("Unknown RPC method: {}", full_method))?;
        let rsp_type = method.output().full_name().to_string();
        let bytes = self.encode_from_json(&rsp_type, json_rsp)?;
        Ok((rsp_type, bytes))
    }

    /// Reads an ExcelConfig/*.bytes table file and returns (TypeName, JsonRowArray)
    #[allow(dead_code)]
    pub fn load_excel_bytes(
        &self,
        bytes_path: impl AsRef<Path>,
    ) -> Result<(String, Vec<Value>), Box<dyn std::error::Error>> {
        let raw = fs::read(bytes_path)?;
        let container = XresDataBlocks::decode(raw.as_slice())?;
        let msg_type = container.data_message_type.clone();

        let mut rows = Vec::with_capacity(container.data_block.len());
        for row_bytes in &container.data_block {
            let row_json = self.decode_to_json(&msg_type, row_bytes)?;
            rows.push(row_json);
        }
        Ok((msg_type, rows))
    }
}