//! Multipart form-data 构建器
//!
//! 用于作业提交时构建 multipart/form-data 请求体。
//! 参考 pku3b 实现，手动构建以确保字段顺序和格式与 Blackboard 兼容。

use rand::Rng;
use std::io::Read;

struct FormField<'a> {
    name: &'a str,
    filename: Option<&'a str>,
    content_type: Option<&'a str>,
    reader: Option<Box<dyn Read + Send + 'static>>,
    data: Option<&'a [u8]>,
}

pub struct MultipartBuilder<'a> {
    boundary: String,
    fields: Vec<FormField<'a>>,
}

impl<'a> MultipartBuilder<'a> {
    pub fn new() -> Self {
        let boundary: String = rand::thread_rng()
            .sample_iter(&rand::distributions::Alphanumeric)
            .take(16)
            .map(char::from)
            .collect();

        Self {
            boundary: format!("----WebKitFormBoundary{boundary}"),
            fields: Vec::new(),
        }
    }

    pub fn add_field(mut self, name: &'a str, data: &'a [u8]) -> Self {
        self.fields.push(FormField {
            name,
            filename: None,
            content_type: None,
            reader: None,
            data: Some(data),
        });
        self
    }

    pub fn add_file<R: Read + Send + 'static>(
        mut self,
        name: &'a str,
        filename: &'a str,
        content_type: &'a str,
        reader: R,
    ) -> Self {
        self.fields.push(FormField {
            name,
            filename: Some(filename),
            content_type: Some(content_type),
            reader: Some(Box::new(reader)),
            data: None,
        });
        self
    }

    pub fn build(mut self) -> anyhow::Result<Vec<u8>> {
        let mut output = Vec::new();
        let dash_boundary = format!("--{}", self.boundary);

        for field in &mut self.fields {
            output.extend_from_slice(dash_boundary.as_bytes());
            output.extend_from_slice(b"\r\n");

            output.extend_from_slice(
                format!("Content-Disposition: form-data; name=\"{}\"", field.name).as_bytes(),
            );
            if let Some(filename) = field.filename {
                output.extend_from_slice(format!("; filename=\"{filename}\"").as_bytes());
            }
            output.extend_from_slice(b"\r\n");

            if let Some(content_type) = field.content_type {
                output.extend_from_slice(format!("Content-Type: {content_type}\r\n").as_bytes());
            }
            output.extend_from_slice(b"\r\n");

            if let Some(data) = field.data {
                output.extend_from_slice(data);
            } else if let Some(reader) = field.reader.as_mut() {
                std::io::copy(reader, &mut output)?;
            }
            output.extend_from_slice(b"\r\n");
        }

        output.extend_from_slice(dash_boundary.as_bytes());
        output.extend_from_slice(b"--\r\n");

        Ok(output)
    }

    pub fn boundary(&self) -> &str {
        &self.boundary
    }
}
