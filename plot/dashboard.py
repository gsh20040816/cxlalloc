import sys

import dash
from dash import Dash, html, dcc, Input, Output, callback
import dash_bootstrap_components as dbc
import plotly.express as px
import polars as pl
from polars import selectors as cs


DF = pl.read_ndjson(sys.argv[1])


TYPE_COL = "col"
TYPE_STORE = "store"
ID_FIGURE = "figure"

COLS = []
CHOICES_INDEPENDENT = [
    {"label": label, "value": value}
    for label, value in [
        ("Set as X-axis", "x"),
        ("Facet along row", "facet_row"),
        ("Facet along column", "facet_column"),
        ("Facet along color", "facet_color"),
        ("Ignore", "ignore"),
    ]
]
CHOICES_DEPENDENT = [
    {"label": label, "value": value}
    for label, value in [
        ("Set as Y-axis", "y"),
        ("Ignore", "ignore"),
    ]
]


class Col:
    def __init__(self, name: str, selector):
        self.name = name
        self.selector = selector

    def store(self):
        return dcc.Store(
            id={"type": TYPE_STORE, "index": self.name},
            storage_type="local",
        )

    # ID used in pattern matching callback
    # https://dash.plotly.com/pattern-matching-callbacks
    def id(self):
        return {"type": TYPE_COL, "index": self.name}


def main():
    ui_control = [html.H2("Control")]
    ui_independent = [html.H2("Independent")]
    ui_dependent = [html.H2("Dependent")]

    for col in flatten(DF):
        if col.name.startswith("output"):
            COLS.append(col)
            ui_dependent.append(
                dbc.Row(
                    [
                        col.store(),
                        dbc.Col(html.Span(col.name)),
                        dbc.Col(
                            dcc.Dropdown(CHOICES_DEPENDENT, id=col.id()),
                        ),
                    ]
                )
            )
            continue

        values = unique(col.selector)

        if len(values) == 1:
            value = values[0]
            if type(value) is bool:
                value = "true" if value else "false"

            ui_control.append(
                dbc.Row(
                    [
                        dbc.Col(html.Span(col.name)),
                        dbc.Col(dcc.Dropdown([value], value=value, disabled=True)),
                    ]
                )
            )
            continue

        COLS.append(col)
        ui_independent.append(
            dbc.Row(
                [
                    col.store(),
                    dbc.Col(html.Span(col.name)),
                    dbc.Col(
                        dcc.Dropdown(
                            CHOICES_INDEPENDENT
                            + [
                                {"label": f"Filter to {value}", "value": value}
                                for value in values.to_list()
                            ],
                            id=col.id(),
                        )
                    ),
                ]
            )
        )

    app = Dash(
        external_stylesheets=[dbc.themes.BOOTSTRAP],
    )
    app.layout = [
        dbc.Row(html.H1(sys.argv[1])),
        dbc.Row(html.Hr()),
        dbc.Row(
            [
                dbc.Col(ui_control),
                dbc.Col(ui_independent),
                dbc.Col(ui_dependent),
            ]
        ),
        dcc.Graph(figure={}, id=ID_FIGURE),
    ]
    app.run(debug=True)


def flatten(df):
    def recurse(columns, namespace, selector):
        select = pl.col if selector is None else lambda col: selector.struct.field(col)

        for col in columns:
            dtype = df.select(select(col)).to_series().dtype
            name = col if namespace == "" else f"{namespace}/{col}"

            if hasattr(dtype, "fields"):
                yield from recurse(
                    [field.name for field in dtype.fields],
                    name,
                    select(col),
                )
            else:
                yield Col(name, select(col))

    yield from recurse(df.columns, "", None)


def unique(selector):
    return DF.select(selector).unique().to_series().sort()


@callback(
    Output({"type": TYPE_STORE, "index": dash.MATCH}, "data"),
    Output({"type": TYPE_COL, "index": dash.MATCH}, "value"),
    Input({"type": TYPE_STORE, "index": dash.MATCH}, "data"),
    Input({"type": TYPE_COL, "index": dash.MATCH}, "value"),
)
def sync_store(store, ui):
    if ui is None:
        return store, store
    else:
        return ui, ui


@callback(
    Output(component_id=ID_FIGURE, component_property="figure"),
    Input({"type": TYPE_STORE, "index": dash.ALL}, "modified_timestamp"),
    dash.State({"type": TYPE_STORE, "index": dash.ALL}, "data"),
)
def update(
    ts,
    values,
):
    if ts is None or any([value is None for value in values]):
        raise dash.exceptions.PreventUpdate

    x = None
    y = None
    facet_row = None
    facet_column = None
    facet_color = None
    filters = []

    # Validate
    for col, value in zip(COLS, values):
        if value == "x":
            if x is not None:
                return {}
            x = col
        elif value == "y":
            if y is not None:
                return {}
            y = col
        elif value == "facet_row":
            if facet_row is not None:
                return {}
            facet_row = col
        elif value == "facet_column":
            if facet_column is not None:
                return {}
            facet_column = col
        elif value == "facet_color":
            if facet_color is not None:
                return {}
            facet_color = col
        elif value == "ignore":
            continue
        elif value is not None:
            filters.append(col.selector == value)

    if x is None or y is None:
        raise dash.exceptions.PreventUpdate

    filtered = DF.filter(*filters)

    sorts = [x.name]
    cols = [
        x.selector.first().alias(x.name),
        y.selector.mean().alias(f"{y.name}_mean"),
        y.selector.std().alias(f"{y.name}_std"),
    ]

    for col in [v for v in [facet_row, facet_column, facet_color] if v is not None]:
        sorts.append(col.name)
        if col.name not in filtered.columns:
            cols.append(col.selector.first().alias(col.name))

    filtered = filtered.group_by(cs.exclude("output")).agg(cols).sort(sorts)

    fig = px.line(
        filtered,
        x=x.name,
        y=f"{y.name}_mean",
        error_y=f"{y.name}_std",
        facet_row=facet_row.name if facet_row is not None else None,
        facet_col=facet_column.name if facet_column is not None else None,
        color=facet_color.name if facet_color is not None else None,
        markers=True,
    )

    fig.update_xaxes(title_text=x.name, tickvals=filtered[x.name].unique())
    fig.update_yaxes(title_text=y.name, autorangeoptions_include=0.0)
    return fig


if __name__ == "__main__":
    main()
