interface ListItem {
  cameoMediaId: string;
  title: string;
  posterUrl?: string;
  position: number;
}

interface ListItemRowProps {
  item: ListItem;
  dragHandleProps?: object;
}

export default function ListItemRow({
  item,
  dragHandleProps,
}: ListItemRowProps) {
  return (
    <div
      data-testid="list-item-row"
      data-cameo-media-id={item.cameoMediaId}
      className="list-item-row"
    >
      {/* List item row stub with drag handle */}
      <span
        className="list-item-drag-handle"
        {...dragHandleProps}
        title="Drag to reorder"
      >
        ⠿
      </span>
      <span className="list-item-position">{item.position + 1}</span>
      {item.posterUrl && (
        <img
          src={item.posterUrl}
          alt={item.title}
          className="list-item-thumbnail"
        />
      )}
      <span className="list-item-title">{item.title}</span>
    </div>
  );
}
