use itertools::Itertools;

fn labels_to_string(iter: DebugUtilsMessengerCallbackLabelIter<'_>) -> String {
    Itertools::intersperse(iter.map(|l| format!("'{}'", l.label_name)), ", ".into()).collect()
}

const fn object_type_to_str(int: i32) -> &'static str {
    match int {
        0 => "UNKNOWN",
        1 => "INSTANCE",
        2 => "PHYSICAL_DEVICE",
        3 => "DEVICE",
        4 => "QUEUE",
        5 => "SEMAPHORE",
        6 => "COMMAND_BUFFER",
        7 => "FENCE",
        8 => "DEVICE_MEMORY",
        9 => "BUFFER",
        10 => "IMAGE",
        11 => "EVENT",
        12 => "QUERY_POOL",
        13 => "BUFFER_VIEW",
        14 => "IMAGE_VIEW",
        15 => "SHADER_MODULE",
        16 => "PIPELINE_CACHE",
        17 => "PIPELINE_LAYOUT",
        18 => "RENDER_PASS",
        19 => "PIPELINE",
        20 => "DESCRIPTOR_SET_LAYOUT",
        21 => "SAMPLER",
        22 => "DESCRIPTOR_POOL",
        23 => "DESCRIPTOR_SET",
        24 => "FRAMEBUFFER",
        25 => "COMMAND_POOL",
        _ => "<unknown>",
    }
}

pub(super) fn debug_messenger(
    severity: DebugUtilsMessageSeverity,
    ty: DebugUtilsMessageType,
    data: DebugUtilsMessengerCallbackData<'_>,
) {
    match severity {
        DebugUtilsMessageSeverity::VERBOSE => {}
        DebugUtilsMessageSeverity::INFO => {}
        _ => {
            // error or warning, so log it

            let mut header = String::from("Vulkan debug message");

            if ty.contains(DebugUtilsMessageType::GENERAL) {
                header += " [General] ";
            }

            if ty.contains(DebugUtilsMessageType::PERFORMANCE) {
                header += " [Performance] ";
            }

            if ty.contains(DebugUtilsMessageType::VALIDATION) {
                header += " [Validation] ";
            }

            let message = format!(
                "{}:\n\
                - Queues: {}\n\
                - Command buffers: {}\n\
                - Objects: {}\n\
                - ID: ({}) {}\n\
                - Message: '{}'",
                header,
                labels_to_string(data.queue_labels),
                labels_to_string(data.cmd_buf_labels),
                Itertools::intersperse(
                    data.objects.map(|o| format!(
                        "'{}' ({}, 0x{:x})",
                        o.object_name.unwrap_or("no label"),
                        object_type_to_str(o.object_type.as_raw()),
                        o.object_handle
                    )),
                    ", ".into()
                )
                .collect::<String>(),
                data.message_id_number,
                data.message_id_name.unwrap_or(""),
                data.message
            );

            if severity == DebugUtilsMessageSeverity::WARNING {
                log::warn!("{}", message);
            } else {
                log::error!("{}", message);
            }
        }
    }
}
