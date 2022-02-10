use std::{io, path::PathBuf, process::Stdio, time::Duration};

use axum::extract::ws::WebSocket;
use bytes_lines_codec::BytesLinesCodec;
use chrono::Utc;
use futures::{SinkExt, StreamExt, TryStreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use telemetry::prelude::*;
use thiserror::Error;
use tokio::{
    process::{Child, ChildStdin, ChildStdout, Command},
    time,
};
use tokio_serde::{
    formats::{Json, SymmetricalJson},
    Framed, SymmetricallyFramed,
};
use tokio_util::codec::{FramedRead, FramedWrite};

use crate::{
    process::{self, ShutdownError},
    server::WebSocketMessage,
    CodeGenerated, CodeGenerationRequest, CodeGenerationResultSuccess, FunctionResult,
    FunctionResultFailure, FunctionResultFailureError, Message, OutputStream,
};

const TX_TIMEOUT_SECS: Duration = Duration::from_secs(2);

pub fn execute(
    lang_server_path: impl Into<PathBuf>,
    lang_server_debugging: bool,
) -> CodeGenerationExecution {
    CodeGenerationExecution {
        lang_server_path: lang_server_path.into(),
        lang_server_debugging,
    }
}

#[derive(Debug, Error)]
pub enum CodeGenerationError {
    #[error("failed to consume the {0} stream for the child process")]
    ChildIO(&'static str),
    #[error("failed to receive child process message")]
    ChildRecvIO(#[source] io::Error),
    #[error("failed to send child process message")]
    ChildSendIO(#[source] io::Error),
    #[error("failed to spawn child process; program={0}")]
    ChildSpawn(#[source] io::Error, PathBuf),
    #[error(transparent)]
    ChildShutdown(#[from] ShutdownError),
    #[error("failed to deserialize json message")]
    JSONDeserialize(#[source] serde_json::Error),
    #[error("failed to serialize json message")]
    JSONSerialize(#[source] serde_json::Error),
    #[error("send timeout")]
    SendTimeout(#[source] tokio::time::error::Elapsed),
    #[error("failed to close websocket")]
    WSClose(#[source] axum::Error),
    #[error("failed to receive websocket message--stream is closed")]
    WSRecvClosed,
    #[error("failed to receive websocket message")]
    WSRecvIO(#[source] axum::Error),
    #[error("failed to send websocket message")]
    WSSendIO(#[source] axum::Error),
    #[error("unexpected websocket message type: {0:?}")]
    UnexpectedMessageType(WebSocketMessage),
}

type Result<T> = std::result::Result<T, CodeGenerationError>;

#[derive(Debug)]
pub struct CodeGenerationExecution {
    lang_server_path: PathBuf,
    lang_server_debugging: bool,
}

impl CodeGenerationExecution {
    pub async fn start(self, ws: &mut WebSocket) -> Result<CodeGenerationServerExecutionStarted> {
        Self::ws_send_start(ws).await?;
        let request = Self::read_request(ws).await?;

        let mut command = Command::new(&self.lang_server_path);
        command
            .arg("codeGeneration")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped());
        if self.lang_server_debugging {
            command.env("DEBUG", "*").env("DEBUG_DEPTH", "5");
        }
        debug!(cmd = ?command, "spawning child process");
        let mut child = command
            .spawn()
            .map_err(|err| CodeGenerationError::ChildSpawn(err, self.lang_server_path.clone()))?;

        let stdin = child
            .stdin
            .take()
            .ok_or(CodeGenerationError::ChildIO("stdin"))?;
        Self::child_send_function_request(stdin, request).await?;

        let stdout = {
            let stdout = child
                .stdout
                .take()
                .ok_or(CodeGenerationError::ChildIO("stdout"))?;
            let codec = FramedRead::new(stdout, BytesLinesCodec::new());
            SymmetricallyFramed::new(codec, SymmetricalJson::default())
        };

        Ok(CodeGenerationServerExecutionStarted { child, stdout })
    }

    async fn read_request(ws: &mut WebSocket) -> Result<CodeGenerationRequest> {
        let request = match ws.next().await {
            Some(Ok(WebSocketMessage::Text(json_str))) => {
                CodeGenerationRequest::deserialize_from_str(&json_str)
                    .map_err(CodeGenerationError::JSONDeserialize)?
            }
            Some(Ok(unexpected)) => {
                return Err(CodeGenerationError::UnexpectedMessageType(unexpected))
            }
            Some(Err(err)) => return Err(CodeGenerationError::WSRecvIO(err)),
            None => return Err(CodeGenerationError::WSRecvClosed),
        };
        Ok(request)
    }

    async fn ws_send_start(ws: &mut WebSocket) -> Result<()> {
        let msg = Message::<CodeGenerationResultSuccess>::Start
            .serialize_to_string()
            .map_err(CodeGenerationError::JSONSerialize)?;

        time::timeout(TX_TIMEOUT_SECS, ws.send(WebSocketMessage::Text(msg)))
            .await
            .map_err(CodeGenerationError::SendTimeout)?
            .map_err(CodeGenerationError::WSSendIO)?;
        Ok(())
    }

    async fn child_send_function_request(
        stdin: ChildStdin,
        request: CodeGenerationRequest,
    ) -> Result<()> {
        let codec = FramedWrite::new(stdin, BytesLinesCodec::new());
        let mut stdin = SymmetricallyFramed::new(codec, SymmetricalJson::default());

        time::timeout(TX_TIMEOUT_SECS, stdin.send(request))
            .await
            .map_err(CodeGenerationError::SendTimeout)?
            .map_err(CodeGenerationError::ChildSendIO)?;
        time::timeout(TX_TIMEOUT_SECS, stdin.close())
            .await
            .map_err(CodeGenerationError::SendTimeout)?
            .map_err(CodeGenerationError::ChildSendIO)?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct CodeGenerationServerExecutionStarted {
    child: Child,
    stdout: Framed<
        FramedRead<ChildStdout, BytesLinesCodec>,
        LangServerCodeGenerationMessage,
        LangServerCodeGenerationMessage,
        Json<LangServerCodeGenerationMessage, LangServerCodeGenerationMessage>,
    >,
}

impl CodeGenerationServerExecutionStarted {
    pub async fn process(self, ws: &mut WebSocket) -> Result<CodeGenerationServerExecutionClosing> {
        let mut stream = self
            .stdout
            .map(|ls_result| match ls_result {
                Ok(ls_msg) => match ls_msg {
                    LangServerCodeGenerationMessage::Output(output) => {
                        Ok(Message::OutputStream(output.into()))
                    }
                    LangServerCodeGenerationMessage::Result(result) => {
                        Ok(Message::Result(result.into()))
                    }
                },
                Err(err) => Err(CodeGenerationError::ChildRecvIO(err)),
            })
            .map(|msg_result: Result<_>| match msg_result {
                Ok(msg) => match msg
                    .serialize_to_string()
                    .map_err(CodeGenerationError::JSONSerialize)
                {
                    Ok(json_str) => Ok(WebSocketMessage::Text(json_str)),
                    Err(err) => Err(err),
                },
                Err(err) => Err(err),
            });

        while let Some(msg) = stream.try_next().await? {
            ws.send(msg).await.map_err(CodeGenerationError::WSSendIO)?;
        }

        Ok(CodeGenerationServerExecutionClosing { child: self.child })
    }
}

#[derive(Debug)]
pub struct CodeGenerationServerExecutionClosing {
    child: Child,
}

impl CodeGenerationServerExecutionClosing {
    pub async fn finish(mut self, mut ws: WebSocket) -> Result<()> {
        let finished = Self::ws_send_finish(&mut ws).await;
        let closed = Self::ws_close(ws).await;
        let shutdown =
            process::child_shutdown(&mut self.child, Some(process::Signal::SIGTERM), None)
                .await
                .map_err(Into::into);
        drop(self.child);

        match (finished, closed, shutdown) {
            // Everything succeeds, great!
            (Ok(_), Ok(_), Ok(_)) => Ok(()),

            // One of the steps failed, return its error
            (Ok(_), Ok(_), Err(err)) | (Ok(_), Err(err), Ok(_)) | (Err(err), Ok(_), Ok(_)) => {
                Err(err)
            }

            // 2/3 steps errored so warn about the lower priority error and return the highest
            // priority
            (Ok(_), Err(err), Err(shutdown)) => {
                warn!(error = ?shutdown, "failed to shutdown child cleanly");
                Err(err)
            }
            (Err(err), Ok(_), Err(shutdown)) => {
                warn!(error = ?shutdown, "failed to shutdown child cleanly");
                Err(err)
            }
            (Err(err), Err(closed), Ok(_)) => {
                warn!(error = ?closed, "failed to cleanly close websocket");
                Err(err)
            }

            // All steps failed so warn about the lower priorities and return the highest priority
            (Err(err), Err(closed), Err(shutdown)) => {
                warn!(error = ?shutdown, "failed to shutdown child cleanly");
                warn!(error = ?closed, "failed to cleanly close websocket");
                Err(err)
            }
        }
    }

    async fn ws_send_finish(ws: &mut WebSocket) -> Result<()> {
        let msg = Message::<CodeGenerationResultSuccess>::Finish
            .serialize_to_string()
            .map_err(CodeGenerationError::JSONSerialize)?;
        time::timeout(TX_TIMEOUT_SECS, ws.send(WebSocketMessage::Text(msg)))
            .await
            .map_err(CodeGenerationError::SendTimeout)?
            .map_err(CodeGenerationError::WSSendIO)?;

        Ok(())
    }

    async fn ws_close(ws: WebSocket) -> Result<()> {
        ws.close().await.map_err(CodeGenerationError::WSClose)
    }
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "protocol", rename_all = "camelCase")]
enum LangServerCodeGenerationMessage {
    Output(LangServerOutput),
    Result(LangServerResult),
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct LangServerOutput {
    execution_id: String,
    stream: String,
    level: String,
    group: Option<String>,
    message: String,
    data: Option<Value>,
}

impl From<LangServerOutput> for OutputStream {
    fn from(value: LangServerOutput) -> Self {
        Self {
            execution_id: value.execution_id,
            stream: value.stream,
            level: value.level,
            group: value.group,
            data: value.data,
            message: value.message,
            timestamp: timestamp(),
        }
    }
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
enum LangServerResult {
    Success(LangServerSuccess),
    Failure(LangServerFailure),
}

impl From<LangServerResult> for FunctionResult<CodeGenerationResultSuccess> {
    fn from(value: LangServerResult) -> Self {
        match value {
            LangServerResult::Success(success) => Self::Success(CodeGenerationResultSuccess {
                execution_id: success.execution_id,
                timestamp: timestamp(),
                data: success.data,
            }),
            LangServerResult::Failure(failure) => Self::Failure(FunctionResultFailure {
                execution_id: failure.execution_id,
                error: FunctionResultFailureError {
                    kind: failure.error.kind,
                    message: failure.error.message,
                },
                timestamp: timestamp(),
            }),
        }
    }
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct LangServerSuccess {
    execution_id: String,
    message: Option<String>,
    data: CodeGenerated,
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct LangServerFailure {
    #[serde(default)]
    execution_id: String,
    error: LangServerFailureError,
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct LangServerFailureError {
    kind: String,
    message: String,
}

fn timestamp() -> u64 {
    // We're going eat any timestamp values that are negative (it is an `i64`) and replace them
    // with 0, which will then safely fit in a `u64` without overflow/underflow
    u64::try_from(std::cmp::max(Utc::now().timestamp(), 0)).expect("timestamp not be negative")
}