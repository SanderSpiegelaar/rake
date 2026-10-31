use rake_config::RakeConfig;
use tao::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};
use wry::WebViewBuilder;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = RakeConfig::new("https://example.com");
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("Rake")
        .with_inner_size(LogicalSize::new(config.width, config.height))
        .build(&event_loop)?;
    let _webview = WebViewBuilder::new().with_url(&config.url).build(&window)?;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        if let Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } = event
        {
            *control_flow = ControlFlow::Exit;
        }
    });
}
