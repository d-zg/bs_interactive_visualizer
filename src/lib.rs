use wasm_bindgen::prelude::*;
use yew::prelude::*;
use plotters::prelude::*;
use plotters_canvas::CanvasBackend;
use statrs::distribution::{LogNormal, Normal, Continuous, ContinuousCDF};
use statrs::statistics::Distribution;

/// The main app component.
#[function_component(App)]
fn app() -> Html {
    // State hooks for the distribution parameters.
    let mu = use_state(|| 0.05);
    let sigma = use_state(|| 0.25);
    let strike = use_state(|| 1.0);

    // A NodeRef to obtain a reference to the canvas element.
    let canvas_ref = use_node_ref();

    // Use an effect that redraws the plot whenever any parameter changes.
    {
        let mu = *mu;
        let sigma = *sigma;
        let strike = *strike;
        let canvas_ref = canvas_ref.clone();
        use_effect_with_deps(
            move |_| {
                if let Some(canvas) = canvas_ref.cast::<web_sys::HtmlCanvasElement>() {
                    if let Err(e) = draw_plot(&canvas, mu, sigma, strike) {
                        web_sys::console::error_1(&e.to_string().into());
                    }
                }
                // No cleanup necessary.
                || ()
            },
            (mu, sigma, strike),
        );
    }

    // Input callbacks to update the state.
    let on_mu_input = {
        let mu = mu.clone();
        Callback::from(move |e: InputEvent| {
            let input: web_sys::HtmlInputElement = e.target_unchecked_into();
            if let Ok(val) = input.value().parse::<f64>() {
                mu.set(val);
            }
        })
    };
    let on_sigma_input = {
        let sigma = sigma.clone();
        Callback::from(move |e: InputEvent| {
            let input: web_sys::HtmlInputElement = e.target_unchecked_into();
            if let Ok(val) = input.value().parse::<f64>() {
                sigma.set(val);
            }
        })
    };
    let on_strike_input = {
        let strike = strike.clone();
        Callback::from(move |e: InputEvent| {
            let input: web_sys::HtmlInputElement = e.target_unchecked_into();
            if let Ok(val) = input.value().parse::<f64>() {
                strike.set(val);
            }
        })
    };

    html! {
        <div style="font-family: sans-serif">
            <h1>{ "Interactive Lognormal PDF & Call Payoff Plot" }</h1>
            <div style="margin-bottom: 1em;">
                <label>{ "Mu: " }</label>
                <input type="number" step="0.01" value={(*mu).to_string()} oninput={on_mu_input} />
            </div>
            <div style="margin-bottom: 1em;">
                <label>{ "Sigma: " }</label>
                <input type="number" step="0.01" value={(*sigma).to_string()} oninput={on_sigma_input} />
            </div>
            <div style="margin-bottom: 1em;">
                <label>{ "Strike: " }</label>
                <input type="number" step="0.01" value={(*strike).to_string()} oninput={on_strike_input} />
            </div>
            <canvas ref={canvas_ref} width="640" height="480"></canvas>
        </div>
    }
}

/// Draws the plot on the provided HTML canvas element.
///
/// This function does the following:
/// 1. Creates a lognormal distribution with the given μ (mu) and σ (sigma).
/// 2. Computes the call option payoff (integral from strike to infinity of (x - strike)*f(x)) 
///    using the closed-form formula:
///       C(K) = exp(mu + sigma²/2) * Phi(d₁) - K * Phi(d₂)
///    where d₁ = (mu - ln(K) + sigma²) / sigma and d₂ = d₁ - sigma.
/// 3. Plots the lognormal density and shades the region for x ≥ strike.
fn draw_plot(
    canvas: &web_sys::HtmlCanvasElement,
    mu: f64,
    sigma: f64,
    strike: f64,
) -> Result<(), Box<dyn std::error::Error>> {
    // Create the lognormal distribution.
    let ln = LogNormal::new(mu, sigma)?;
    // Compute the call payoff components.
    let d1 = (mu - strike.ln() + sigma * sigma) / sigma;
    let d2 = d1 - sigma;
    let norm = Normal::new(0.0, 1.0)?;
    let call_payoff = (mu + sigma * sigma / 2.0).exp() * norm.cdf(d1) - strike * norm.cdf(d2);

    // Define x-axis range.
    let xmin = 0.001;
    let xmax = ln.mean().unwrap_or(1.0) * 3.0;
    let n_points = 1000;
    let dx = (xmax - xmin) / n_points as f64;

    // Generate PDF data.
    let mut density_data = Vec::with_capacity(n_points + 1);
    for i in 0..=n_points {
        let x = xmin + i as f64 * dx;
        density_data.push((x, ln.pdf(x)));
    }

    // Create a canvas backend for Plotters.
    let backend = CanvasBackend::with_canvas_object(canvas.clone()).unwrap();
    let root = backend.into_drawing_area();
    root.fill(&WHITE)?;

    // Determine a good y-axis range.
    let max_pdf = density_data.iter().map(|&(_, y)| y).fold(0.0, f64::max);
    let mut chart = ChartBuilder::on(&root)
        .caption("Interactive Lognormal PDF & Call Payoff Region", ("sans-serif", 20))
        .margin(20)
        .x_label_area_size(40)
        .y_label_area_size(40)
        .build_cartesian_2d(xmin..xmax, 0.0..max_pdf)?;

    chart.configure_mesh().x_desc("x").y_desc("PDF").draw()?;

    // Draw the density curve.
    chart.draw_series(LineSeries::new(density_data.clone(), &BLUE))?;

    // Shade the integration region (x ≥ strike).
    let call_region: Vec<(f64, f64)> = density_data
        .into_iter()
        .filter(|(x, _)| *x >= strike)
        .collect();
    let mut area_points: Vec<(f64, f64)> = Vec::new();
    area_points.push((strike, 0.0));
    area_points.extend(call_region);
    area_points.push((xmax, 0.0));
    chart.draw_series(std::iter::once(Polygon::new(
        area_points,
        RED.mix(0.2).filled(),
    )))?;

    // Draw a vertical line at the strike price.
    chart.draw_series(std::iter::once(PathElement::new(
        vec![(strike, 0.0), (strike, max_pdf)],
        &BLACK,
    )))?;

    // Annotate the plot with the computed call payoff.
    chart.draw_series(std::iter::once(Text::new(
        format!("Call Payoff = {:.4}", call_payoff),
        (strike * 1.05, max_pdf * 0.8),
        ("sans-serif", 15),
    )))?;

    root.present()?;
    Ok(())
}

#[wasm_bindgen(start)]
pub fn run_app() {
    yew::Renderer::<App>::new().render();
}
